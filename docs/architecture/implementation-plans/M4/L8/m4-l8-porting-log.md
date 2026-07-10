# M4-L8 Porting Log

Status: active

Parent plans:

- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`

Closeout status for the L8A-L8G cleanup pass:

- Resolved in code: clippy exit gate, bootstrap failure -> `Failed`, L8G
  bootstrap file boundary, checkpoint-boundary replay idempotence, lossy
  health classification, strict WAL-tail rejection, quarantine/table validation
  before WAL repair, cache open admission, idempotent close facts, capability
  required/missing facts, structured open outcome facts, class-prefixed stable
  error codes, checkpoint timestamp guard catch-up, timeline mismatch mapping,
  typed recovery-visibility failure reporting, and positive
  capability-order/source-guard tests.
- Explicitly deferred: exhaustive crash/fault/fuzz closeout, localfs recovery
  integration expansion, full maintenance/retention/quarantine/repair outcomes,
  durable close drain/sync outcomes, and the remaining named L8F/L8G/L8D/L8E
  matrix rows that require later L8H-L8P machinery.
- Ordering note: sections below reflect the order closeout entries were appended
  during implementation. The parent implementation plan remains the canonical
  slice ordering guide.

## L8A - Lifecycle Scaffold

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/commit/config.rs`
- `crates/storage-next/src/commit/error.rs`
- `crates/storage-next/src/commit/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/commit_runtime_properties.rs`
- `crates/storage-next/tests/commit_runtime_source_guard.rs`
- `crates/storage-next/tests/common/mod.rs`
- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/result.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Lifecycle state vocabulary: new, opening, recovering, open, closing, closed,
  and failed.
- Storage mode vocabulary: cache, durable local standard, durable local always,
  and object durable candidate.
- Storage open plan and open outcome facts.
- Recovery health shape: healthy, degraded, and failed.
- Recovery fault categories for manifest, snapshot, WAL, table object, inherited
  layer, IO, quarantine inventory, and timeline mismatch facts.
- Maintenance task vocabulary for flush, checkpoint, WAL truncation,
  compaction, materialization, snapshot pruning, retention, quarantine, purge,
  repair, and health collection.
- Retention, quarantine, and close fact shells.
- Lower-layer source-chain preservation through `LifecycleError::source()` and
  stable V1 class-prefixed `LifecycleError::code()` strings such as
  `invalid_argument.lifecycle.config`,
  `failed_precondition.lifecycle.state`, and
  `corruption.lifecycle.recovery`.

### Raw Health And Fact Vocabulary

- Lifecycle enums are marked `#[non_exhaustive]` so later L8/L9 slices can add
  fields or variants without changing current call sites by accident.
- `StorageOpenOutcome` now carries backend capabilities, database id, codec id,
  recovered max commit version, checkpoint/WAL/table/quarantine recovery facts,
  L7 bootstrap facts, and raw `LifecycleStats` in addition to the original mode,
  disposition, recovered visible version, health, and maintenance-ready facts.
- `MaintenanceOutcome` and `CloseOutcome` have reserved structured facts for
  later maintenance/close slices: recovery health, affected-object counts,
  reclaimed bytes, retryability, close fact, close effects, and raw stats.
- `LifecycleError::CapabilityMismatch` carries both required and missing
  backend capabilities; timeline replay mismatches and strict WAL-tail repair
  rejections have dedicated lifecycle error codes.

### Intentional Changes

- The scaffold is crate-private. The crate root remains `mod lifecycle;`.
- Config uses explicit enums for close timeout and lossy recovery policy.
- Lossy recovery is disabled by default and must be explicit before an open plan
  can request lossy fallback.
- Cache-mode open plans cannot request durable recovery fallback.
- Cache-mode open outcomes cannot claim a recovered durable visible version.
- The generated lifecycle property route is a scaffold contract only; it does
  not open storage or mutate lower layers.

### Retired From V1 L8

- Product open policy and public open wording.
- Public maintenance command vocabulary.
- Primitive reconstruction callbacks.
- Product recovery advice.
- Follower refresh behavior.
- IPC or multi-process product behavior.
- StrataHub behavior.
- Product value, graph, vector, search, embedding, or inference DTOs.

### Deferred By Owner Slice

- Lifecycle state transition validation: L8B.
- Backend and service capability validation: L8C.
- Cache-mode open and close baseline: L8D.
- Durable local open/create service assembly: L8E.
- Recovery orchestration and L7 replay/bootstrap: L8F-L8G.
- Maintenance executor and task queue behavior: L8H.
- Flush, checkpoint, WAL truncation, compaction, and materialization scheduling:
  L8I-L8K.
- Retention, quarantine, purge, and repair orchestration: L8L-L8M.
- Close ordering, drain, sync, and guard release: L8N.
- Fault, crash, fuzz, sensitivity, and closeout inventory: L8O-L8P.

### Tests Added

- Module-local scaffold tests in `src/lifecycle/tests/mod.rs`.
- Source-boundary guard in `tests/lifecycle_source_guard.rs`.
- Generated scaffold property harness in `tests/lifecycle_properties.rs`.
- Hidden testkit route `check_lifecycle_scaffold_contract`.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Product/engine import | Import engine, product, StrataHub, or follower modules in lifecycle source | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Raw filesystem/env access | Import `std::fs`, `Path`, `File`, mmap, `OpenOptions`, or `std::env` | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Lower layer imports lifecycle | Import `crate::lifecycle` from backend, format, service, table, branch, or commit source | `lower_layers_do_not_import_lifecycle_upward` |
| Public lifecycle surface | Add unscoped `pub` item in lifecycle source | `lifecycle_stays_crate_private` |
| Config zero limit | Accept zero config limits | `lifecycle_config_rejects_zero_limits` and generated lifecycle properties |
| Error code collapse | Remove stable lifecycle error code mapping | `lifecycle_error_display_and_source_chain_are_typed` |
| Source-chain collapse | Drop lower-layer source from `LifecycleError` | `lifecycle_error_display_and_source_chain_are_typed` |

### Verification

Commands to run for L8A:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8Q Durable Table Manifest Format

### Shipped Files

- `crates/storage-next/src/format/table_manifest.rs`
- `crates/storage-next/src/format/table_manifest/tests.rs`
- `crates/storage-next/src/format/table_manifest/tests/canonical.rs`
- `crates/storage-next/src/format/table_manifest/tests/constructor_object.rs`
- `crates/storage-next/src/format/table_manifest/tests/corruption.rs`
- `crates/storage-next/src/format/table_manifest/tests/inheritance_provenance_extension.rs`
- `crates/storage-next/src/format/mod.rs`
- `crates/storage-next/src/format/fuzzing.rs`
- `crates/storage-next/src/format/tests.rs`
- `crates/storage-next/src/testkit/format_fuzz.rs`
- `crates/storage-next/tests/format_golden.rs`
- `crates/storage-next/tests/table_format_source_guard.rs`
- `crates/storage-next/fuzz/Cargo.toml`
- `crates/storage-next/fuzz/fuzz_targets/format_table_manifest.rs`
- `crates/storage-next/fuzz/corpus/format_table_manifest/valid-empty`
- `crates/storage-next/fuzz/corpus/format_table_manifest/valid-owned-levels`
- `crates/storage-next/fuzz/corpus/format_table_manifest/valid-inherited-layer`
- `crates/storage-next/fuzz/corpus/format_table_manifest/valid-materialization-provenance`
- `crates/storage-next/fuzz/corpus/format_table_manifest/valid-unknown-optional-extension`
- `crates/storage-next/fuzz/corpus/format_table_manifest/bad-checksum`
- `crates/storage-next/fuzz/corpus/format_table_manifest/future-version`
- `crates/storage-next/fuzz/corpus/format_table_manifest/truncated-table-entry`
- `crates/storage-next/testdata/goldens/storage-format-v1/table-manifest-empty.hex`
- `crates/storage-next/testdata/goldens/storage-format-v1/table-manifest-owned-levels.hex`
- `crates/storage-next/testdata/goldens/storage-format-v1/table-manifest-inherited-layers.hex`
- `crates/storage-next/testdata/goldens/storage-format-v1/table-manifest-materialization-provenance.hex`
- `crates/storage-next/testdata/goldens/storage-format-v1/table-manifest-extension-section.hex`

### Preserved As Storage Vocabulary

- Branch-scoped table manifest bytes with branch id, optional branch
  generation, manifest sequence, owned levels, inherited layers, table
  identities, local table object names, row/block/byte counts, commit ranges,
  optional timestamp ranges, physical/internal bounds, provenance, and optional
  extension sections.
- Deterministic constructor canonicalization and fail-closed decoder validation
  for checksums, format version, canonical ordering, duplicate table identities,
  duplicate object names, bounded counts, non-overlapping lower-level physical
  ranges, and unknown required extension flags.
- Primitive-neutral extension vocabulary: optional sections can be preserved on
  rewrite, but product/workflow extension kinds are rejected at the format
  boundary.

### Intentional Changes

- Branch ids remain opaque `BranchId` atoms at the durable-format boundary.
  All-zero branch ids are accepted here because the core type does not reserve
  that value as an empty sentinel.
- Sparse owned levels are accepted by the format. Branch-runtime policy can
  reject or normalize sparse levels before publication, but the byte codec does
  not add a branch topology rule.

### Retired From This Slice

- Table manifest publication or replacement through lifecycle services.
- Recovery from table manifests into branch state.
- Flush watermark advancement from table manifests.
- Table-object retention, quarantine, or deletion.
- Database-manifest pointers to branch table manifests.

### Deferred By Owner Slice

- L8R publishes table manifests and recovers branch state from them.
- L8S consumes table manifests for durable table-object reachability proof.
- L8T decides when table-manifest-covered flushes can shorten WAL replay.
- L8U/L8V/L8X consume the format for rewrite durability, retention-aware
  pruning, and cache/lazy-read budget work.

### Tests Added

- Constructor and object-name validation tests from the L8Q test plan,
  including zero sequence, invalid generation, duplicate level/table/object,
  invalid facts/bounds, path-like object names, wrong object family, branch
  mismatch, and table-object level mismatch.
- Canonical encoding tests from the L8Q test plan, including owned-table,
  inherited-layer, materialization-provenance round trips, level
  canonicalization, L0 precedence preservation, L1+ physical-range ordering,
  inherited-layer order preservation, and semantically distinct L0 order bytes.
- Golden-vector tests for empty, owned-level, inherited-layer,
  materialization-provenance, and extension-section manifests.
- Corruption/version/robustness tests for bad magic, pre-V1/future versions,
  truncated header/table/layer, trailing bytes, checksum mismatch, count and
  length overflow, invalid UTF-8, reserved flags, random bytes, noncanonical
  bytes, and the structured mutation matrix.
- Inherited-layer, provenance, and extension-section tests covering active,
  materializing, materialized statuses; duplicate inherited sources; all
  table-provenance variants; missing materialization fork; required sections;
  optional section preservation; duplicate/invalid/product/primitive section
  identifiers; and runtime-handle absence.
- Source guard tests named for raw IO, backend service, lifecycle execution,
  engine/product, StrataHub, primitive module, product-workflow, and
  lower-layer lifecycle-policy boundaries.
- Fuzz target registration plus corpus inventory for all eight required
  semantic seeds.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Checksum validation removed | `crates/storage-next/src/format/table_manifest.rs` | Ignore stored CRC mismatch | `table_manifest_rejects_checksum_mismatch` |
| Duplicate identity accepted | `crates/storage-next/src/format/table_manifest.rs` | Remove `table_identity` set check | `table_manifest_rejects_duplicate_identity_and_object` |
| Duplicate object accepted | `crates/storage-next/src/format/table_manifest.rs` | Remove `table_object` set check | `table_manifest_rejects_duplicate_identity_and_object` |
| Cross-branch object accepted | `crates/storage-next/src/format/table_manifest.rs` | Remove branch component validation | `table_manifest_rejects_cross_branch_table_objects` |
| Malformed table-object layout accepted | `crates/storage-next/src/format/table_manifest.rs` | Accept non-`tables/<branch>/lNNNN/<table>` object names or mismatched level component | `table_manifest_rejects_wrong_table_object_shape_or_level` |
| L1 overlap accepted | `crates/storage-next/src/format/table_manifest.rs` | Change `previous_last >= first` to `>` | `table_manifest_rejects_l1_overlap_and_bad_order` |
| L0 precedence erased | `crates/storage-next/src/format/table_manifest.rs` | Sort L0 by table identity instead of explicit order | `different_l0_order_encodes_differently` |
| Physical-range policy weakened | `crates/storage-next/src/format/table_manifest.rs` | Check L1+ internal ranges instead of physical ranges | `table_manifest_rejects_l1_plus_overlap` |
| Future version accepted | `crates/storage-next/src/format/table_manifest.rs` | Treat any version as current | `table_manifest_rejects_corruption_and_future_version` |
| Count bounds removed | `crates/storage-next/src/format/table_manifest.rs` | Remove max-count checks before allocation | `table_manifest_decode_large_counts_does_not_allocate_unbounded_memory` |
| Inherited-layer status dropped | `crates/storage-next/src/format/table_manifest.rs` | Encode all inherited-layer statuses as Active | `table_manifest_preserves_materializing_status` |
| Materialization provenance dropped | `crates/storage-next/src/format/table_manifest.rs` | Encode replacement provenance as generic compaction | `table_manifest_preserves_materialization_replacement_provenance` |
| Required extension accepted | `crates/storage-next/src/format/table_manifest.rs` | Ignore required extension flag | `table_manifest_rejects_unknown_required_extension` |
| Product extension accepted | `crates/storage-next/src/format/table_manifest.rs` | Remove reserved extension vocabulary check | `table_manifest_rejects_reserved_extension_vocabulary` |
| Source boundary leak | `crates/storage-next/src/format/table_manifest.rs` | Import lifecycle/service/layout directly | `table_manifest_format_source_stays_below_service_and_lifecycle_layers` |

### Verification

Commands run for L8Q:

```bash
cargo test -p strata-storage-next --locked --lib table_manifest
cargo test -p strata-storage-next --locked --lib format::
cargo test -p strata-storage-next --features testkit --locked --lib format_fuzz
cargo test -p strata-storage-next --locked --test format_golden
cargo test -p strata-storage-next --locked --test table_format_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test testkit_boundary
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bin format_table_manifest
cargo clippy -p strata-storage-next --all-targets --locked -- -D warnings
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
rustfmt --check crates/storage-next/src/format/mod.rs crates/storage-next/src/format/fuzzing.rs crates/storage-next/src/format/tests.rs crates/storage-next/src/format/table_manifest.rs crates/storage-next/src/testkit/format_fuzz.rs crates/storage-next/tests/format_golden.rs crates/storage-next/tests/table_format_source_guard.rs crates/storage-next/fuzz/fuzz_targets/format_table_manifest.rs
git diff --check
```

`cargo fmt --package strata-storage-next --check` was also run. It still reports
pre-existing formatting drift in lifecycle/testkit files outside this slice; the
L8Q touched Rust files pass `rustfmt --check` directly.

## L8R Table Manifest Publication And Recovery

### Shipped Files

- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/mod.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/mod.rs`
- `crates/storage-next/src/lifecycle/tests/table_manifest_recovery.rs`
- `crates/storage-next/src/lifecycle/tests/flush.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/recovery.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Branch-scoped durable table manifests are the only trusted table-object
  reachability source during recovery. Recovery does not list
  `tables/<branch>/...` and infer live tables from object names.
- Every manifest-listed table object is opened through the table-object reader
  service and validated against manifest facts and bounds before branch-state
  install.
- Durable flush publishes the table object, validates/reopens it, installs it
  into branch state, records catalog facts, and then publishes the branch table
  manifest. Publication failure after install is reported as health debt and
  does not claim WAL truncation safety.
- Table-manifest recovery is conservative: it does not advance flush watermarks,
  truncate WAL, or shorten replay. Those proofs remain owned by later
  table-manifest-backed retention work.

### Intentional Changes

- Explicitly lossy recovery degrades, rather than hard-failing, for corrupt
  table manifests and missing manifest-listed table objects. It does not install
  the untrusted table graph.
- Missing table manifest for an otherwise empty branch remains healthy.
- Cache mode has no table-manifest publication or recovery surface.

### Retired From This Slice

- Directory or prefix scanning as a recovery truth source.
- Filename-only table manifests from the old segmented engine.
- Direct table-object deletion, quarantine mutation, or retention decisions.
- Public/product branch workflow recovery.

### Deferred By Owner Slice

- Table-manifest-backed flush-watermark proof and replay shortening: L8T.
- Safe table-object retention/quarantine decisions from all branch manifests:
  L8S/L8M.
- Durable compaction/materialization output publication: L8U.
- Branch list/delete/clear/fork-at-history completion: L8Y.

### Tests Added

- Typed table-manifest service tests for absent, present, corrupt, branch
  mismatch, canonical create/replace, invalid publish metadata, source-error
  preservation, and database-manifest byte rejection.
- Durable table catalog tests for exact duplicate acceptance, identity/object
  conflict rejection, and sequence catch-up after recovered manifest records.
- Durable flush integration tests proving table manifests are published after
  table install, preserve existing reachable tables, report publication failure
  as health debt, surface uncertainty, and are absent in cache mode.
- Recovery tests for owned L0/L1+ install, inherited layers, materializing layer
  status, orphan object invisibility, corrupt manifest strict failure, lossy
  corrupt-manifest degradation, missing listed object strict failure, lossy
  missing-object degradation, corrupt listed objects, fact/bounds mismatches,
  WAL replay conservatism, and latest-read preservation after WAL tail replay.
- Generated recovery counters for table-manifest publish/recover, corrupt
  manifests, missing objects, object mismatches, and orphan invisibility.
- Source guards preventing prefix-scanning reachability, WAL truncation from
  table-manifest publication, cache-mode table-manifest imports, and upward
  lifecycle imports into manifest services.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| R1 | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Publish table manifest before table-object validation/install | `durable_flush_publishes_table_manifest_after_table_install` |
| R2 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Allow catalog to build manifest from missing durable table facts | `durable_table_catalog_accepts_exact_duplicate_and_rejects_conflicts` |
| R3 | `crates/storage-next/src/lifecycle/table_manifest.rs` | On corrupt manifest, list table prefix and install objects as live tables | `strict_recovery_rejects_corrupt_table_manifest_without_loading_orphans` and `table_manifest_recovery_does_not_list_table_prefix_for_reachability` |
| R4 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Treat missing listed table object as a generic table decode failure | `strict_recovery_rejects_missing_manifest_listed_table_object` |
| R5 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Ignore row-count, byte-count, block-count, commit, or bounds mismatch | `recovery_rejects_table_object_fact_and_bounds_mismatches` |
| R6 | `crates/storage-next/src/lifecycle/recovery.rs` | Advance replay start or flush watermark from table-manifest coverage | `table_manifest_recovery_does_not_change_wal_replay_start` |
| R7 | `crates/storage-next/src/lifecycle/cache.rs` | Import table-manifest service into cache runtime | `cache_mode_does_not_import_table_manifest_service` |
| R8 | `crates/storage-next/src/branch/state.rs` | Bypass branch recovery validation for recovered manifest tables | `recovery_installs_manifest_owned_front_and_sorted_tables` and `recovery_installs_inherited_layers_from_manifest` |
| R9 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Drop lower-layer source chain on table reader error | `strict_recovery_rejects_corrupt_manifest_listed_table_object` |
| R10 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Import raw IO or prefix listing into recovery path | `lifecycle_source_does_not_import_engine_product_or_raw_io` and `table_manifest_recovery_does_not_list_table_prefix_for_reachability` |
| R11 | `crates/storage-next/src/lifecycle/recovery.rs` | Ignore table manifest when checkpoint is also recovered | `recovery_rejects_checkpoint_table_manifest_duplicate_internal_key_conflict` |
| R12 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Accept a recovered manifest with a smaller sequence than catalog state | `durable_table_catalog_rejects_manifest_sequence_regress` |
| R13 | `crates/storage-next/src/lifecycle/recovery.rs` | Match `is_lossy_table_manifest_recovery_error` on a `LowerLayer` reason string instead of a typed marker variant | `lossy_recovery_reports_missing_manifest_listed_table_object` |
| R14 | `crates/storage-next/src/lifecycle/table_manifest.rs` | Drop the volatile-rewrite-output reason from `entry_for` and reuse the generic publication-failed reason | `build_manifest_reports_volatile_rewrite_output_with_clear_error` |

### Audit-Round Findings And Fixes

| Finding | Resolution |
|---|---|
| Recovery silently skipped the table manifest whenever a checkpoint was recovered, leaving Recovery Protocol rule 9 untested. | Recovery now always stages the manifest and runs `preflight_table_manifest_with_checkpoint` to compare internal-key bytes; the typed `TableManifestCheckpointConflict` rejects byte divergence while exact duplicates pass. |
| `entry_for` returned a generic publication-failed reason that did not name the underlying cause when branch state contained a volatile rewrite output. | The reason string now states "branch table state contains volatile rewrite output without durable catalog entry"; a focused unit test asserts it. |
| `is_lossy_table_manifest_recovery_error` and the related fault classifiers matched on the literal reason string from `branch_error`, so any rewording silently routed branch-runtime failures to the strict-only path. | Introduced `LifecycleError::TableManifestBranchInstallFailed`; lifecycle paths build it through `LifecycleError::table_manifest_branch_install_failed_with`, and the lossy classifier matches the variant, not a reason string. |
| `LifecycleDurableTableCatalog::record_manifest` accepted any sequence on recovery, allowing a later flush to publish below an earlier on-disk manifest. | Recovery rejects manifests whose `manifest_sequence` is below `next_manifest_sequence` with a typed publication-failed error. |
| Generated recovery counters did not cover `table_manifest_missing`, `table_object_corrupt`, `checkpoint_manifest_conflict`, or `cache_manifest_unsupported`. | Added the four counters to `LifecycleRecoveryContractOutcome` and exercised them through new `check_*` helpers in `testkit/lifecycle/recovery.rs`. |

### Verification

Commands run for L8R:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table_manifest
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_manifest_recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8W Memory And Cache Budget Enforcement

### Shipped Files

- `crates/storage-next/src/lifecycle/budget.rs`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/close.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/tests/budget.rs`
- `crates/storage-next/src/lifecycle/tests/budget_runtime.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/src/testkit/lifecycle/budget.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/src/table/tests/cache.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Implementation Notes

- Added explicit `StorageRuntimeBudget` limits for block cache, table reader,
  active mutable state, frozen mutable state, maintenance queue, generated
  artifacts, and manifest/catalog metadata.
- Added a database-local `StorageBudgetLedger` with RAII reservations, stable
  pool names, raw usage facts, pressure facts, and typed
  `StorageBudgetExceeded` errors.
- Threaded budget configuration through `LifecycleConfig`, selected budget
  facts through `StorageOpenOutcome`, and live runtime usage through cache and
  durable runtime snapshot helpers.
- Wired cache/durable commit admission to active mutable bytes, maintenance
  rotation to frozen byte/count limits, and maintenance enqueue admission to
  queue byte/count limits with coalescing before budget checks.
- Counted active maintenance tasks in queue budget snapshots and follow-on
  enqueue admission, so a running task still consumes its reservation.
- Added generated-artifact admission to checkpoint snapshot construction before
  snapshot publication; durable maintenance and close checkpoint paths pass the
  runtime budget through the same helper.
- Wired flush, durable rewrite publication, and table-manifest publication
  through generated-artifact, reader, and manifest-catalog budget checks.
- Wired checkpoint recovery decode through the generated-artifact budget before
  row-section decode/install so oversized checkpoint payloads fail closed before
  allocating decoded rows.
- Bound table cache configuration to the explicit block-cache budget; zero
  block-cache budget maps to disabled cache rather than a hidden default.
- Added a generated lifecycle budget contract that exercises budget accept and
  reject routes, reservation release on success and failure, cache eviction,
  reader rejection, active-row rejection, artifact deferral, maintenance queue
  rejection, low-memory smoke, and database-local isolation from input-derived
  scripts.
- Added source guards preventing host-memory probing, hidden process-global
  cache state, product resource-policy imports, object-cleanup imports, and
  primitive/StrataHub dependencies in the budget module.
- Follow-up review fixes:
  - Recovery now admits manifest-listed readers through the table reader
    budget. `recover_manifest_table` calls `require_table_reader_budget`
    against the per-table object byte count before invoking
    `reader_service.open_reader`, matching the admission rule already
    applied in flush, compaction, materialization, and rewrite
    publication. The budget threads through
    `stage_table_manifest_for_branch` →
    `recover_table_manifest_for_branch` →
    `recovery_request_from_manifest` →
    `recover_manifest_levels` / `recover_manifest_inherited_layer` →
    `recover_manifest_table`. Prior to this fix, a recovered manifest
    referencing a table larger than `table_reader_bytes` would silently
    open the reader on a low-memory profile.
  - Documented the admission-check vs RAII reservation split in the
    `budget` module header: V1 production paths use admission checks for
    `TableReader`, `GeneratedArtifact`, and `ManifestCatalog`, while the
    ledger still exposes RAII reservations for tests and future
    block-range reader work. `ActiveMutable`, `FrozenMutable`, and
    `MaintenanceQueue` usage continues to derive from runtime state.

### Preserved As Storage Vocabulary

- Budget pressure is reported as raw storage usage and severity facts:
  `Normal`, `Evicting`, `DeferOptionalMaintenance`,
  `RejectOptionalWork`, and `RejectMutatingAdmission`.
- Budget rejection uses the stable code
  `resource_exhausted.lifecycle.storage_budget` and preserves pool,
  requested bytes/count, used bytes/count, limits, and reason.
- Low-memory profile values are explicit test fixtures. Storage does not
  inspect host RAM, CPU count, device model, environment variables, or OS
  probes to infer a profile.

### Deferred Within The Budget Workstream

- Lazy object-backed table reads and block/range reader reservations remain in
  L8X.
- Pinned-cache accounting remains deferred because storage-next does not yet
  expose a pin/unpin cache contract. The shipped tests cover zero-capacity,
  oversized uncached reads, bounded eviction, shrink pressure, stats, and
  database-local identity isolation.
- Full lazy-reader budget scripts remain with the reader/cache slice. The
  shipped generated budget contract is intentionally limited to current
  lifecycle/table-cache budget APIs.
- Public profile selection and user-facing pressure/diagnostic rendering
  remain L9 boundary work.

### Tests Added

- `storage_budget_accepts_explicit_low_memory_profile`
- `storage_budget_accepts_zero_optional_block_cache`
- `storage_budget_rejects_zero_mandatory_active_pool`
- `storage_budget_rejects_total_smaller_than_required_pools`
- `storage_budget_rejects_overflowing_pool_sum`
- `storage_budget_reports_all_pool_limits`
- `budget_reservation_acquire_and_release`
- `budget_reservation_exact_fit_succeeds`
- `budget_reservation_explicit_release_clears_usage`
- `budget_reservation_failed_acquire_does_not_change_usage`
- `budget_reservation_nested_failure_releases_outer`
- `budget_reservation_overflow_rejects`
- `budget_reservation_rejects_one_byte_over_limit`
- `budget_reservation_drop_releases_usage`
- `budget_ledger_is_database_local`
- `budget_reservation_rejects_one_byte_over_limit_without_usage_change`
- `budget_stats_are_deterministic`
- `budget_pressure_reports_pool_usage_and_limit`
- `cache_open_reports_selected_storage_budget`
- `storage_budget_rejects_reader_count_zero_when_readers_required`
- `storage_budget_rejects_frozen_table_count_zero_when_flush_enabled`
- `storage_budget_profile_does_not_probe_host_memory`
- `low_memory_profile_does_not_apply_hidden_minimum_cache`
- `reader_open_exact_budget_succeeds`
- `reader_count_limit_rejects_extra_reader`
- `reader_open_over_budget_rejects_before_decode`
- `reader_open_failure_releases_reservation`
- `reader_drop_releases_reservation`
- `reader_budget_counts_concurrent_readers`
- `reader_budget_error_names_table_identity`
- `reader_budget_cache_mode_and_durable_mode_match`
- `active_append_under_budget_succeeds`
- `active_append_over_budget_rejects_before_mutation`
- `active_budget_reports_approximate_bytes_after_commit`
- `rotate_active_under_frozen_budget_succeeds`
- `rotate_active_over_frozen_count_budget_rejects_before_state_change`
- `rotate_active_over_frozen_byte_budget_rejects_before_state_change`
- `flush_releases_frozen_budget_after_install`
- `flush_failure_keeps_frozen_budget_reserved`
- `maintenance_queue_count_limit_rejects_extra_task`
- `maintenance_queue_byte_limit_rejects_large_task`
- `maintenance_coalescing_happens_before_budget_reservation`
- `maintenance_cancel_releases_reservation`
- `maintenance_close_drain_releases_reservations`
- `maintenance_active_task_holds_reservation`
- `maintenance_task_failure_releases_reservation`
- `maintenance_optional_task_deferred_under_pressure`
- `maintenance_mandatory_close_task_admitted_under_optional_pressure`
- `maintenance_budget_pressure_added_to_outcome`
- `cache_flush_generated_artifact_budget_rejects_before_install`
- `cache_flush_table_reader_budget_rejects_before_install`
- `checkpoint_encode_over_budget_rejects_before_snapshot_publish`
- `flush_artifact_exact_budget_succeeds`
- `compaction_artifact_over_budget_defers_before_publish`
- `materialization_artifact_over_budget_defers_before_publish`
- `recovery_decode_over_budget_fails_closed`
- `partial_artifact_failure_releases_budget`
- `artifact_actual_size_reconciles_with_estimate`
- `artifact_budget_reports_output_bytes`
- `artifact_budget_does_not_truncate_wal_or_delete_objects`
- `table_manifest_publication_checks_manifest_catalog_budget_before_publish`
- `metadata_budget_stats_report_catalog_bytes`
- `recovery_mandatory_metadata_budget_failure_is_typed`
- `quarantine_inventory_over_budget_rejects_before_vector_allocation`
- `retention_graph_over_budget_defers_optional_reclaim`
- `metadata_pressure_blocks_optional_maintenance_first`
- `corrupt_metadata_does_not_allocate_unbounded_memory`
- `low_memory_profile_opens_cache_runtime`
- `low_memory_profile_opens_durable_runtime_on_test_backend`
- `low_memory_profile_opens_durable_runtime_on_memory_backend`
- `low_memory_profile_allows_small_commit_read_flush_checkpoint_close`
- `low_memory_profile_defers_large_compaction_artifact`
- `low_memory_profile_zero_cache_still_reads_uncached`
- `low_memory_profile_reports_pressure_without_product_policy`
- `low_memory_profile_does_not_auto_detect_host_memory`
- `reader_budget_recovery_decode_rejects_large_table`
- `reader_budget_fails_closed_for_large_whole_object_reads_until_lazy_reads_ship`
- `low_memory_profile_rejects_large_whole_table_reader_until_lazy_reads`
- `active_append_failure_does_not_advance_commit_visibility`
- `cache_and_durable_active_budget_behavior_match`
- `manifest_decode_rejects_large_section_count_before_allocation`
- Generated/property test:
  `lifecycle_property_harness_runs_budget_contract`.
- Table cache tests:
  `zero_capacity_table_cache_does_not_store`,
  `small_cache_serves_oversized_block_uncached`,
  `table_cache_respects_capacity_after_insert`,
  `table_cache_eviction_effort_is_bounded`,
  `table_cache_shrink_records_pressure`,
  `table_cache_stats_include_hits_misses_entries_bytes`,
  `table_cache_keys_use_table_identity_not_path`,
  `two_runtime_caches_are_isolated`.
- Source guards:
  `memory_budget_does_not_probe_host_memory`,
  `memory_budget_does_not_use_process_global_cache`,
  `memory_budget_does_not_probe_host_memory_or_use_global_cache`,
  `memory_budget_does_not_import_product_resource_policy`,
  `memory_budget_does_not_import_raw_io`,
  `memory_budget_does_not_import_backend_delete_or_quarantine`,
  `memory_budget_does_not_import_object_cleanup_boundaries`,
  `memory_budget_does_not_import_stratahub`,
  `memory_budget_does_not_import_primitive_modules`,
  `memory_budget_code_and_fixture_names_do_not_use_milestone_labels`.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| W1: Clamp zero cache to default capacity | `crates/storage-next/src/lifecycle/budget.rs` | Return enabled cache from zero block-cache budget | `storage_budget_accepts_zero_optional_block_cache` and `zero_capacity_table_cache_does_not_store` |
| W2: Use process-global cache or budget state | `crates/storage-next/src/lifecycle/budget.rs` / table cache | Replace database-local state with static/global state | `budget_ledger_is_database_local`, `two_runtime_caches_are_isolated`, or source guard |
| W3: Open reader before reserving bytes | `crates/storage-next/src/lifecycle/flush.rs` / `rewrite_publication.rs` | Skip `require_table_reader_budget` before opening generated tables | `reader_open_over_budget_rejects_before_decode`, `cache_flush_table_reader_budget_rejects_before_install`, or `reader_budget_cache_mode_and_durable_mode_match` |
| W4: Leak reservation on publish failure | `crates/storage-next/src/lifecycle/budget.rs` / `flush.rs` | Remove release or mutate frozen state on failed publication | `budget_reservation_nested_failure_releases_outer`, `partial_artifact_failure_releases_budget`, `maintenance_task_failure_releases_reservation`, and `flush_failure_keeps_frozen_budget_reserved` |
| W5: Ignore frozen byte budget on rotate | `crates/storage-next/src/lifecycle/budget.rs` | Do not pass active bytes into projected frozen usage | `rotate_active_over_frozen_byte_budget_rejects_before_state_change` |
| W6: Allocate duplicate queue reservation before coalescing | `crates/storage-next/src/lifecycle/cache.rs` / `durable/maintenance.rs` | Run budget check before coalesce detection | `maintenance_coalescing_happens_before_budget_reservation` |
| W7: Decode manifest count before budget check | `crates/storage-next/src/lifecycle/table_manifest.rs` / `recovery.rs` | Skip manifest-catalog/generated-artifact admission before publish/recover catalog growth | `table_manifest_publication_checks_manifest_catalog_budget_before_publish`, `recovery_decode_over_budget_fails_closed`, and `corrupt_metadata_does_not_allocate_unbounded_memory` |
| W8: Report product write-stall wording | `crates/storage-next/src/lifecycle/budget.rs` | Import product policy/resource vocabulary | `memory_budget_does_not_import_product_resource_policy` |
| W9: Probe host memory | `crates/storage-next/src/lifecycle/budget.rs` | Import `std::env`, `sysinfo`, or `/proc` helpers | `memory_budget_does_not_probe_host_memory` |
| W10: Let generated usage exceed limit | `crates/storage-next/src/lifecycle/flush.rs` / `checkpoint.rs` / `rewrite_publication.rs` | Skip generated-artifact admission before publication | `cache_flush_generated_artifact_budget_rejects_before_install`, `checkpoint_encode_over_budget_rejects_before_snapshot_publish`, `compaction_artifact_over_budget_defers_before_publish`, and `materialization_artifact_over_budget_defers_before_publish` |

### Verification

Commands run for the implementation pass (all passed):

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table::tests::cache
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib branch::tests
cargo test -p strata-storage-next --locked --lib lifecycle::tests::budget
cargo test -p strata-storage-next --locked --lib lifecycle::tests::budget_runtime
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8X Lazy Object-Backed Table Reads

Status: implemented as bounded range-backed materialized open; full
branch-resident cursor laziness remains gated by the branch table row-slice
contract.

### Shipped Files

- `crates/storage-next/src/format/table/mod.rs`
- `crates/storage-next/src/format/table/artifact.rs`
- `crates/storage-next/src/table/reader.rs`
- `crates/storage-next/src/table/tests/reader.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/lifecycle/tests/table_manifest_recovery.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Implementation Notes

- Added table-format helpers for metadata-only decode from header, footer,
  index-frame, and properties-frame ranges without changing the physical table
  byte format.
- Added a per-data-block decode helper that validates the touched data frame
  against its index entry.
- Changed `ImmutableTableReader::open_source` to read header, footer, index,
  properties, and then bounded data-block ranges instead of performing a
  single whole-object read.
- Changed `TableObjectReaderService::open_reader` to route durable objects
  through the range-backed table source. Backend range-read failures still
  preserve the backend source chain.
- Preserved the byte-slice eager reader path for table fixtures and in-memory
  callers.
- Kept table-manifest recovery reader admission at the materialized table-object
  byte count. Even though durable object open uses bounded ranges, the current
  branch table contract still materializes all rows before installation, so
  metadata-only budget admission would undercount memory.
- Added source guards that forbid full-object durable reads, raw IO, path cache
  identity, process-global cache state, product/primitive imports, cleanup
  mutation, and milestone labels in the lazy-reader path.

### Current Boundary

- Branch state still stores `BranchOwnedTable` with a row-slice reader contract,
  and L6 validation, compaction, checkpoint, manifest publication, and
  materialization code still call `table.rows()`. Because of that contract,
  recovered table objects are opened by bounded ranges but their rows are still
  materialized before installation into branch state.
- Corrupt data blocks still fail during materialized open rather than at first
  point/range query. Query-scoped corruption reporting requires the same
  branch-resident lazy cursor contract change.
- A fully branch-resident lazy cursor requires changing the L6 branch table
  contract so validation and reads can operate through cursors/facts without
  requiring `&[TableRow]` at install time. That is intentionally not hidden by
  this porting entry.
- Metadata-only open uses `decode_table_footer_metadata`, which intentionally
  skips the table-wide footer CRC because the full byte stream is not loaded.
  Footer-field bit-flips are caught downstream by per-frame CRC checks and the
  index/properties/header cross-reference validation in
  `validate_metadata_against_header_and_footer`. The eager byte-slice path
  (`decode_immutable_table`) still verifies the table-wide CRC for fixtures
  and in-memory callers.
- `TableObjectReaderService::require_exact_bytes` keeps a single full-object
  range read (`read_all_for_exact_match`) for publish-dedup equality checks.
  This is sanctioned and scoped to `service/table.rs`; the new
  `lazy_open_path_does_not_perform_full_object_reads` source guard forbids the
  same pattern in `table/reader.rs` and `lifecycle/table_manifest.rs`.

### Tests Updated

- `immutable_reader_opens_table_source_and_maps_source_failures`
- `immutable_reader_bytes_and_source_paths_are_identical_for_queries`
- `table_object_byte_source_enforces_capabilities_and_exact_ranges`
- `table_object_reader_materialized_open_reads_expected_bounded_ranges`
- `table_object_reader_opens_published_object_through_range_source`
- `cache_mode_can_use_eager_reader_without_durable_claim`
- `table_object_reader_allows_missing_metadata_capability`
- `table_object_reader_rejects_missing_range_capability_before_io`
- `table_object_reader_distinguishes_read_decode_and_fact_errors`
- `table_object_reader_routes_corruption_to_table_errors`
- `table_object_reader_rejects_corrupt_index_properties_and_count_mismatch`
- `table_object_reader_rejects_corrupt_data_block_payload_on_materialized_open`
- `table_object_reader_rejects_corrupt_footer_length_fields`
- `reader_budget_recovery_decode_rejects_materialized_table_over_budget`
- `reader_budget_rejects_below_whole_object_while_rows_are_materialized`
- `low_memory_profile_rejects_large_materialized_table_reader`
- `recovery_opens_manifest_table_with_bounded_range_reads`
- `recovery_with_large_manifest_table_does_not_read_full_object`
- `recovery_range_backed_reader_preserves_branch_read_parity`
- `lazy_reader_does_not_full_read_durable_object_on_open`
- `lazy_open_path_does_not_perform_full_object_reads`
- `lazy_reader_does_not_import_raw_io`
- `lazy_reader_does_not_use_path_cache_identity_or_global_cache`
- `lazy_reader_does_not_import_product_or_cleanup_policy`
- `lazy_reader_code_and_fixture_names_do_not_use_milestone_labels`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| X1: Full object read during open | `crates/storage-next/src/service/table.rs` | Reintroduce whole-object read in `open_reader` | `lazy_reader_does_not_full_read_durable_object_on_open`, `lazy_open_path_does_not_perform_full_object_reads`, `table_object_reader_materialized_open_reads_expected_bounded_ranges` |
| X2: Skip index for point lookup | — | Out of scope; branch-resident cursor laziness is deferred (see Current Boundary). |
| X3: Cursor reads past upper bound | — | Out of scope; bounded cursor laziness is deferred (see Current Boundary). |
| X4: Cache by path or object name only | `crates/storage-next/src/table/reader.rs` | Add path-derived cache key or global cache state | `lazy_reader_does_not_use_path_cache_identity_or_global_cache` |
| X5: Skip block budget release on decode error | — | Out of scope; touched-block budget is deferred. |
| X6: Treat corrupt untouched block as open failure | — | Out of scope; untouched-block corruption classification is deferred. |
| X7: Treat corrupt touched block as absence | `crates/storage-next/src/format/table/artifact.rs` | Skip header/footer/index/properties/data-block validation | `table_object_reader_routes_corruption_to_table_errors`, `table_object_reader_rejects_corrupt_index_properties_and_count_mismatch`, `table_object_reader_rejects_corrupt_data_block_payload_on_materialized_open`, `table_object_reader_rejects_corrupt_footer_length_fields` |
| X8: Drop tombstone/TTL metadata during decode | `crates/storage-next/src/format/table/data.rs` | Strip tombstone/TTL fields from per-block decode | `table_object_reader_matches_byte_reader_for_queries_and_row_shapes` |
| X9: Reintroduce process-global cache | `crates/storage-next/src/table/reader.rs` | Use `OnceLock`/`lazy_static` for cache state | `lazy_reader_does_not_use_path_cache_identity_or_global_cache` |
| X10: Import raw file IO in reader | `crates/storage-next/src/table/reader.rs` | Add `std::fs` or `OpenOptions` imports | `lazy_reader_does_not_import_raw_io` |
| Materialized budget undercount | `crates/storage-next/src/lifecycle/table_manifest.rs` | Admit recovered manifest tables by metadata estimate or skip budget check | `reader_budget_recovery_decode_rejects_materialized_table_over_budget`, `reader_budget_rejects_below_whole_object_while_rows_are_materialized`, `low_memory_profile_rejects_large_materialized_table_reader` |
| Collapse backend read error | `crates/storage-next/src/service/table.rs` | Wrap `TableObjectReadError::Backend` as object-neutral table error | `table_object_reader_distinguishes_read_decode_and_fact_errors` |
| Misread manifest table object during recovery | `crates/storage-next/src/lifecycle/table_manifest.rs` | Reopen manifest-listed tables through a full-object read or unbounded range | `recovery_opens_manifest_table_with_bounded_range_reads`, `recovery_with_large_manifest_table_does_not_read_full_object` |

### Verification

Commands run for the implementation pass (all passed):

```bash
cargo test -p strata-storage-next --locked --lib table::tests::reader
cargo test -p strata-storage-next --locked --lib service::table
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_manifest_recovery
cargo test -p strata-storage-next --locked --lib table::tests::cache
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test table_runtime_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo fmt --package strata-storage-next --check
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
cargo test -p strata-storage-next --locked --test lifecycle_source_guard lazy_reader
```

## L8U - Durable Rewrite Publication

Status: runtime implementation and test suite landed

### Shipped Files

- `crates/storage-next/src/branch/mod.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/rewrite_publication.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs`
- `crates/storage-next/src/lifecycle/tests/compaction/publication_plan.rs`
- `crates/storage-next/src/lifecycle/tests/compaction/remaining.rs`
- `crates/storage-next/src/testkit/lifecycle/rewrite.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Durable rewrite publication is expressed as table-object publication,
  object-backed reader validation, branch-runtime install, durable table-catalog
  update, and branch table-manifest publication.
- Compaction and materialization outcomes distinguish volatile completion,
  checkpoint-required completion, durable table-manifest-backed completion, and
  post-install manifest debt.
- Rewrite publication failures preserve source chains and use dedicated
  lifecycle error codes for failed, uncertain, and orphaned publication
  windows.
- Durable output object names and retained input identifiers flow into
  maintenance outcomes for later reachability, quarantine, and retention work.

### Intentional Changes

- Branch runtime now exposes prepared compaction/materialization outputs so L8
  can publish the exact L5/L6 output bytes before installing them. The existing
  branch install paths still own candidate validation, identity validation,
  stale-candidate rejection, materialization handle checks, and atomic state
  mutation.
- Durable rewrite publication lives in a dedicated lifecycle module. The
  generic compaction scheduler remains free of table-object services, preserving
  the existing cache-mode and branch-runtime source boundaries.
- Catalog updates are staged on a cloned durable table catalog before branch
  install. If output objects were published but catalog validation rejects their
  facts, the operation fails before branch mutation and reports orphaned output
  objects.
- Reopen, descriptor, branch-table, and provenance failures after object
  publication are treated as orphaned-output windows rather than ordinary
  pre-install failures.
- Manifest publication failure after branch install is forward-progress debt:
  the branch remains rewritten, the durable table catalog keeps the output
  facts, and the maintenance outcome reports checkpoint-required health debt.

### Retired From This Slice

- Row pruning, tombstone pruning, TTL pruning, and retained-history policy.
- Table-object deletion, quarantine, purge, or reclaim.
- WAL truncation or flush-watermark advancement from rewrite publication.
- Public compaction/materialization commands.
- Object-store or distributed durability.

### Deferred By Owner Slice

- L8V adds proof-gated row pruning on top of the durable rewrite path.
- L8W/L8X add memory-budget and lazy-reader behavior.
- L8Y handles branch lifecycle hardening.
- L8Z completes policy/closeout hardening before the public storage boundary.

### Tests Added

- `durable_compaction_publishes_manifest_after_install`
- `durable_compaction_manifest_failure_reports_debt_after_install`
- `durable_materialization_publishes_manifest_after_layer_removal`
- `durable_compaction_rejects_existing_output_with_conflicting_bytes`
- `durable_materialization_retry_after_manifest_debt_publishes_manifest`
- `durable_compaction_publishes_output_before_manifest_and_reopens_before_install`
- `durable_compaction_publishes_output_before_install`
- `durable_compaction_reopens_output_before_install`
- `durable_compaction_validates_output_facts_before_install`
- `durable_compaction_installs_only_after_all_outputs_validate`
- `durable_compaction_manifest_includes_outputs_and_excludes_replaced_inputs`
- `durable_compaction_manifest_includes_outputs`
- `durable_compaction_manifest_excludes_replaced_inputs`
- `durable_compaction_catalog_marks_replaced_inputs_retained`
- `durable_compaction_output_identities_are_retry_stable`
- `durable_compaction_no_candidate_is_deferred_without_publication`
- `durable_compaction_no_candidate_is_deferred`
- `durable_materialization_manifest_includes_replacements_and_removes_inherited_layer`
- `durable_materialization_binds_handle_before_output_publish`
- `durable_materialization_publishes_replacement_before_layer_removal`
- `durable_materialization_reopens_replacement_before_install`
- `durable_materialization_validates_replacement_facts`
- `durable_materialization_manifest_removes_inherited_layer`
- `durable_materialization_manifest_includes_replacements`
- `durable_materialization_preserves_child_local_precedence`
- `durable_materialization_retry_after_removed_layer_uses_source_identity`
- `durable_materialization_rejects_stale_layer_index_task`
- `durable_compaction_preserves_reads_tombstones_timestamps_and_ttl_rows`
- `durable_compaction_preserves_latest_reads`
- `durable_compaction_preserves_history_reads`
- `durable_compaction_preserves_prefix_scans`
- `durable_compaction_preserves_range_scans`
- `durable_compaction_preserves_tombstones`
- `durable_compaction_preserves_ttl_expired_rows_under_keep_all`
- `durable_compaction_preserves_commit_timestamps`
- `durable_materialization_preserves_reads_and_fork_gate`
- `durable_materialization_preserves_latest_reads`
- `durable_materialization_preserves_history_reads`
- `durable_materialization_preserves_fork_version_gate`
- `rewrite_output_publish_failure_leaves_reads_unchanged`
- `rewrite_output_publish_uncertain_reports_health_debt`
- `rewrite_output_publish_uncertain_names_possibly_visible_object`
- `rewrite_output_reopen_failure_leaves_reads_unchanged`
- `rewrite_output_reopen_failure_leaves_reads_unchanged_and_names_orphan`
- `rewrite_output_fact_mismatch_leaves_reads_unchanged`
- `rewrite_install_failure_after_publish_names_orphan_outputs`
- `rewrite_install_failure_after_publish_does_not_delete_outputs`
- `rewrite_manifest_publish_failure_after_install_keeps_new_reads_visible`
- `rewrite_manifest_publish_failure_after_install_reports_manifest_debt`
- `rewrite_manifest_publish_uncertain_after_install_reports_debt`
- `rewrite_manifest_publish_uncertain_after_install_reports_uncertainty`
- `rewrite_retry_after_manifest_failure_reuses_catalog_entries`
- `rewrite_retry_after_output_publish_collision_rejects_conflict`
- `rewrite_stale_candidate_after_publish_fails_without_resurrection`
- `recovery_after_durable_compaction_uses_manifest_outputs`
- `recovery_after_durable_materialization_uses_manifest_replacements`
- `recovery_after_manifest_publish_failure_uses_previous_manifest_or_wal`
- `recovery_after_output_publish_before_install_ignores_orphan_output`
- `recovery_after_install_before_manifest_records_health_debt`
- `recovery_rejects_corrupt_rewrite_output_listed_by_manifest`
- `recovery_rejects_missing_rewrite_output_listed_by_manifest`
- `recovery_preserves_reads_after_wal_tail_replay`
- `durable_rewrite_completion_does_not_persist_flush_watermark_or_truncate_wal`
- `durable_rewrite_completion_does_not_directly_persist_flush_watermark`
- `durable_rewrite_completion_does_not_directly_truncate_wal`
- `durable_rewrite_manifest_facts_can_build_flush_coverage_candidate`
- `durable_rewrite_manifest_failure_cannot_build_flush_coverage_candidate`
- `durable_rewrite_checkpoint_debt_reduced_only_after_manifest_success`
- `durable_rewrite_manifest_success_can_build_flush_coverage_candidate`
- `durable_rewrite_does_not_delete_or_quarantine_replaced_or_orphaned_objects`
- `durable_rewrite_does_not_delete_replaced_inputs`
- `durable_rewrite_does_not_quarantine_replaced_inputs`
- `durable_rewrite_does_not_delete_published_orphan_outputs`
- `durable_rewrite_does_not_prune_old_versions`
- `durable_rewrite_does_not_prune_tombstones`
- `durable_rewrite_does_not_prune_ttl_expired_rows`
- `durable_rewrite_does_not_call_quarantine_service`
- `durable_rewrite_does_not_call_purge`
- `durable_rewrite_rejects_cache_durable_publication_request`
- `durable_rewrite_rejects_before_open`
- `durable_rewrite_rejects_while_closing`
- `durable_rewrite_rejects_empty_output_seed`
- `durable_rewrite_rejects_path_like_output_seed`
- `durable_rewrite_rejects_pruning_policy_without_retention_proof`
- `durable_rewrite_uses_ordinary_maintenance_admission`
- `durable_rewrite_releases_admission_after_publish_failure`
- `lifecycle_table_rewrite_compaction_integration`
- `lifecycle_table_rewrite_materialization_integration`
- `lifecycle_property_harness_runs_table_rewrite_contract`
- `lifecycle_rewrite_publication_avoids_cleanup_pruning_and_product_dependencies`
- `cache_rewrite_path_does_not_import_table_object_publication`
- `durable_rewrite_publication_does_not_import_raw_io`
- `durable_rewrite_publication_does_not_import_backend_delete`
- `durable_rewrite_publication_does_not_import_quarantine_mutation`
- `durable_rewrite_publication_does_not_import_purge`
- `durable_rewrite_publication_does_not_import_row_pruning_policy`
- `durable_rewrite_publication_does_not_import_engine_or_product_crates`
- `durable_rewrite_publication_does_not_import_stratahub`
- `durable_rewrite_publication_does_not_import_primitive_modules`

Existing compaction/materialization tests continue to cover cache behavior,
checkpoint-debt durable behavior, materialization handle binding, stale
candidate rejection, read parity, pressure facts, and source-chain preservation.
The lifecycle source guards now exercise both the generic table-rewrite boundary
through `lifecycle_table_rewrite_source_uses_branch_runtime_boundaries` and the
durable rewrite-publication boundary through
`lifecycle_rewrite_publication_avoids_cleanup_pruning_and_product_dependencies`.
The generated lifecycle rewrite contract now drives real durable compaction and
materialization routes, including output publication, object-backed reopen,
install-after-publish, manifest-after-install, pre-install publication failure,
post-install manifest failure, install-failed-after-publish, orphan-output
reporting, and no-pruning counters. The dedicated publication-plan module
contains one-to-one tests for the required request/admission, publication,
materialization, read-parity, fault-window, recovery, watermark-boundary, and
no-cleanup rows from the test plan. Test names that would have embedded
architecture labels were renamed to `flush_coverage` equivalents to keep labels
out of Rust code.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Skip output publication | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Install prepared branch outputs without publishing table objects | `durable_compaction_publishes_manifest_after_install` |
| Skip output reopen | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Build branch tables from bytes without object-backed reader validation | `durable_compaction_publishes_manifest_after_install` |
| Publish manifest before install | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Publish branch table manifest before L6 install | `durable_compaction_publishes_manifest_after_install` |
| Ignore manifest publish failure | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Return clean durable completion when table-manifest replacement fails | `durable_compaction_manifest_failure_reports_debt_after_install` |
| Use naked materialization index | `crates/storage-next/src/lifecycle/compaction.rs` | Drop bound materialization handles from maintenance tasks | `queued_materialization_uses_bound_source_after_layer_reindex` |
| Import table service into scheduler | `crates/storage-next/src/lifecycle/compaction.rs` | Add table-object publication imports to the generic scheduler | `lifecycle_table_rewrite_source_uses_branch_runtime_boundaries` |
| Trust conflicting pre-existing output | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Accept `PreconditionFailed` without byte-for-byte validation | `durable_compaction_rejects_existing_output_with_conflicting_bytes` |
| Skip manifest publication for no-output materialization retry | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Return `LayerAlreadyMaterialized` without publishing the branch table manifest | `durable_materialization_retry_after_manifest_debt_publishes_manifest` |
| Hide uncertain output names | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Drop the possibly-visible object from uncertain publication errors | `durable_compaction_rejects_existing_output_with_conflicting_bytes` |
| Fail before install without preserving old reads | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Mutate branch state after table-object publication fails | `rewrite_output_publish_failure_leaves_reads_unchanged` |
| Treat corrupt reopened output as installed | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Skip object-backed reopen error handling after publication | `rewrite_output_reopen_failure_leaves_reads_unchanged_and_names_orphan` |
| Let orphan output become live after recovery | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Recover table objects that were published without manifest reachability | `recovery_after_output_publish_before_install_ignores_orphan_output` |
| Advance flush watermark or truncate WAL during rewrite | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Persist flush coverage or truncate WAL from rewrite completion | `durable_rewrite_completion_does_not_persist_flush_watermark_or_truncate_wal` |
| Delete or quarantine replaced/orphaned table objects | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Call cleanup services from durable rewrite publication | `durable_rewrite_does_not_delete_or_quarantine_replaced_or_orphaned_objects` |
| Drop generated durable rewrite coverage | `crates/storage-next/src/testkit/lifecycle/rewrite.rs` | Stop bumping publication/reopen/install/manifest/failure counters | `lifecycle_property_harness_runs_table_rewrite_contract` |
| Import cleanup or pruning from rewrite publication | `crates/storage-next/src/lifecycle/rewrite_publication.rs` | Add retention, quarantine, WAL truncation, or product dependencies | `lifecycle_rewrite_publication_avoids_cleanup_pruning_and_product_dependencies` |

### Verification

Commands run for the L8U implementation pass:

```bash
cargo fmt --package strata-storage-next --check
cargo check -p strata-storage-next --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
git diff --check
```

## L8N - Close And Shutdown Ordering

### Shipped Files

- `crates/storage-next/src/lifecycle/durable/close.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/cache.rs`
- `crates/storage-next/src/lifecycle/tests/close.rs`
- `crates/storage-next/src/lifecycle/tests/durable.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance.rs`
- `crates/storage-next/src/testkit/lifecycle/close.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md`

### Preserved As Storage Vocabulary

- Close remains a storage lifecycle transition with `Requested`,
  `RetryPending`, `Complete`, and `AlreadyClosed` facts.
- Durable close reports commit quiesce, maintenance drain, durable sync, writer
  guard release, idempotent retry, and close stats through `CloseOutcome`.
- Close timeout is represented by the stable
  `deadline_exceeded.lifecycle.close` code.
- WAL close failures preserve the lower-layer service source chain.
- Writer guard release remains RAII-backed but is now explicit in durable close
  ownership and observable through reacquire behavior.

### Intentional Changes

- Durable close now lives in a dedicated durable close module rather than the
  recovery bootstrap module.
- Durable runtime close cancels cancelable pending maintenance, drains
  drain-required maintenance, acquires commit quiesce, closes/syncs the WAL,
  releases the writer guard, and transitions to `Closed`.
- Close with an active commit guard records retry-pending state and returns a
  typed timeout instead of silently waiting or succeeding.
- WAL sync failure leaves the runtime in retryable closing state and keeps the
  writer guard held until a successful retry.
- Durable services now store the writer guard as optional ownership so close can
  release it exactly once.
- Cache close remains volatile and does not import durable services.

### Retired From V1 L8N

- Product close callbacks, primitive freeze hooks, IPC/server shutdown, and
  public database handle release.
- Background worker thread shutdown.
- Raw filesystem close/fsync code in lifecycle.
- Retention, purge, snapshot pruning, or WAL truncation implicitly started by
  close. Only already-queued drain-required maintenance can run during close.

### Deferred By Owner Slice

- Public close API and product error mapping: L9/engine.
- Crash and fuzz close assurance: L8O/L8P.
- Multi-process lease renewal or handoff beyond the existing writer guard:
  later durable/object-backend work.
- Branch deletion and clear policy during close: later branch lifecycle work.
- Backend-reported writer-guard release failure: deferred past V1.
  `release_writer_guard` on the durable runtime is a take-and-drop of an
  in-memory handle; `LocalFsBackend::acquire_writer_lock` returns a
  guard whose Drop only releases the OS advisory lock. Neither path can
  surface a typed failure today. The close-time contract therefore only
  covers "missing writer guard at release" (`release_writer_guard`
  returning `false`), not "backend rejected the release call." Post-V1
  object-backend work that introduces lease-handoff semantics will need
  to extend `BackendWriterGuard` with a fallible-release hook and wire
  the matching typed close error through the close path; the closeout
  inventory will gain a new scenario at that point.
- Persistent close-time `RecoveryHealth` snapshot in the database manifest
  payload: deferred past V1. The M3 format freeze locks the manifest to
  `database_id`, `codec_id`, `active_wal_segment`, `snapshot_watermark`,
  `snapshot_id`, `flushed_through_commit_id` — no health field is added.
  Session-observed degradation already lives in its source-of-truth disk
  state (quarantine inventory mismatches, orphan snapshots, partial
  publication windows), so the next open's recovery re-walks that state
  and rederives the same `RecoveryHealth` from scratch. The close-time
  hook (`force_final_manifest_fsync_on_health_change` at
  `crates/storage-next/src/lifecycle/durable/close.rs`) instead issues
  one final `PublishMode::Replace` of the existing manifest bytes when
  health changed — same payload, but the publish exercises the backend's
  durable-write path one more time so any pending `fdatasync` on the
  manifest file is flushed before the writer guard releases. Test
  `durable_close_force_syncs_manifest_when_health_changed` asserts both
  parts of this contract: exactly one Replace, and byte-identical
  payload across the operation. Adding a true persistent health field
  is a future format-version bump (post-V1) that needs coordinated
  golden-vector and migration work.

### Tests Added

- `durable_close_syncs_log_releases_writer_guard_and_is_idempotent`
- `durable_close_calls_wal_close_in_always_mode`
- `durable_close_does_not_report_complete_with_unresolved_durable_gate`
- `durable_close_does_not_truncate_wal_prune_snapshots_or_purge_quarantine_implicitly`
- `durable_reopen_can_acquire_writer_guard_after_close`
- `commit_after_close_requested_rejects_before_version_allocation`
- `durable_close_timeout_while_commit_guard_active_is_retryable`
- `durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable`
- `durable_close_log_sync_failure_preserves_writer_guard_for_retry`
- `cache_close_cancels_cancelable_pending_work`
- `cache_close_cancels_ordinary_pending_work_before_closed`
- `close_drain_preserves_task_order`
- `close_retry_after_drain_failure_does_not_rerun_completed_tasks`
- `maintenance_executor_drain_error_keeps_task_pending_for_retry`
- `lifecycle_close_contract_covers_shutdown_categories`
- `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache`
- The remaining close-shutdown test-plan inventory is represented directly by
  plan-named close, cache, maintenance, and durable tests. Note: two tests
  were renamed during the round-3 assurance pass:
  `durable_close_persists_final_health_fact_when_dirty` →
  `durable_close_force_syncs_manifest_when_health_changed` (reflects the
  V1 deferral that health is not persisted into the manifest payload —
  see deferral note above), and
  `cache_close_rejects_or_drains_drain_required_work_by_policy` →
  `cache_close_drains_drain_required_work` (cache now actively drains
  drain-required tasks rather than rejecting). The test plan was updated
  in lock-step with the renames.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Commit admission after close | `crates/storage-next/src/lifecycle/state.rs` | Admit commits while state is closing | `durable_close_timeout_while_commit_guard_active_is_retryable` / existing closed-commit tests |
| Active guard ignored | `crates/storage-next/src/lifecycle/durable/close.rs` | Skip `try_begin_quiesce` failure | `durable_close_timeout_while_commit_guard_active_is_retryable` |
| Drain task quiesce error loses retry | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove a failed drain-required task permanently | `durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable` / `maintenance_executor_drain_error_keeps_task_pending_for_retry` |
| WAL sync failure marked clean | `crates/storage-next/src/lifecycle/durable/close.rs` | Ignore `WalService::close` error | `durable_close_log_sync_failure_preserves_writer_guard_for_retry` |
| Writer guard released before sync | `crates/storage-next/src/lifecycle/durable/close.rs` | Release writer guard before WAL close | `durable_close_log_sync_failure_preserves_writer_guard_for_retry` |
| Writer guard not released | `crates/storage-next/src/lifecycle/durable/close.rs` | Skip guard release on successful close | `durable_close_syncs_log_releases_writer_guard_and_is_idempotent` |
| Unresolved durable gate ignored | `crates/storage-next/src/lifecycle/durable/close.rs` | Continue to WAL close despite unresolved durable commit | `durable_close_does_not_report_complete_with_unresolved_durable_gate` |
| Double close repeats durable sync | `crates/storage-next/src/lifecycle/durable/close.rs` | Run close phases again after `Closed` | `durable_close_syncs_log_releases_writer_guard_and_is_idempotent` |
| Ordinary close-canceled work survives close | `crates/storage-next/src/lifecycle/cache.rs` | Leave ordinary/cancelable tasks pending after cache close | `cache_close_cancels_ordinary_pending_work_before_closed` / `cache_close_cancels_cancelable_pending_work` |
| Generated close counters removed | `crates/storage-next/src/testkit/lifecycle/close.rs` | Do not increment retry/drain/quiesce/sync counters | `lifecycle_close_contract_covers_shutdown_categories` / `lifecycle_property_harness_runs_scaffold_contract` |
| Close logic moved into bootstrap | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Call close/drain/sync from bootstrap | `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache` |
| Cache close calls durable services | `crates/storage-next/src/lifecycle/cache.rs` | Call WAL close or release writer guard in cache close | `lifecycle_durable_close_stays_out_of_assembly_bootstrap_and_cache` |

### Verification

Commands run for L8N:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --locked --lib lifecycle::tests::close
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib commit::tests::guard
cargo test -p strata-storage-next --locked --lib service::wal
plan='docs/architecture/implementation-plans/M4/L8/l8n-close-shutdown-ordering-test-plan.md'; missing=0; for name in $(perl -nE 'while(/`([a-z][a-z0-9_]+)`/g){say $1}' "$plan" | sort -u); do if ! rg -q "fn $name\b|$name" crates/storage-next/src/lifecycle crates/storage-next/src/testkit crates/storage-next/tests docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md; then echo "$name"; missing=$((missing+1)); fi; done; echo missing=$missing
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8O - Generated, Fault, And Crash Assurance

Status: implemented

### Shipped Files

- `crates/storage-next/src/testkit/lifecycle/script.rs`
- `crates/storage-next/src/testkit/lifecycle/fault.rs`
- `crates/storage-next/src/testkit/lifecycle/crash.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_faults.rs`
- `crates/storage-next/tests/lifecycle_fuzz_inventory.rs`
- `crates/storage-next/tests/crash_recovery.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `crates/storage-next/fuzz/Cargo.toml`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_maintenance.rs`
- `crates/storage-next/fuzz/fuzz_targets/lifecycle_retention.rs`
- `crates/storage-next/fuzz/corpus/lifecycle_recovery/*`
- `crates/storage-next/fuzz/corpus/lifecycle_maintenance/*`
- `crates/storage-next/fuzz/corpus/lifecycle_retention/*`
- `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8o-generated-fault-crash-assurance-test-plan.md`

### Preserved As Storage Vocabulary

- Generated assurance remains storage-shaped: lifecycle state, storage mode,
  recovery health, checkpoint/flush/WAL watermarks, table-object reachability,
  quarantine/purge facts, maintenance queue facts, and close retryability.
- Fault coverage reports lifecycle error codes, lower-layer source chains,
  retryability, health debt, and affected storage families.
- Crash assurance reports durable phase-family coverage without introducing
  product crash handlers or public recovery wording.
- Fuzz contracts route arbitrary bytes into lifecycle recovery, maintenance,
  and retention contracts without panicking on corrupt inputs.

### Intentional Changes

- Added a composed generated lifecycle script contract that aggregates the
  existing family contracts and asserts input-derived route counters separately
  from canonical smoke routes.
- Added lightweight generated model facts for visible/checkpoint/flush
  watermarks, durable log records, snapshot/table/quarantine/reclaim counts,
  validation rejections, degraded health, and close state.
- Added fault-window and crash-window testkit contracts so integration tests can
  assert phase-family coverage without duplicating all unit fixtures.
- Registered lifecycle fuzz targets for recovery, maintenance, and retention,
  each calling a distinct contract function.
- Added named non-empty seed corpora for each lifecycle fuzz target.
- Extended source guards to ensure assurance code remains in testkit/tests/fuzz,
  production lifecycle code does not import testkit or fuzz helpers, crash tests
  are feature-gated, and generated properties assert input-derived counters.

### Retired From V1 L8O

- Product crash supervisors, process managers, IPC/server shutdown, primitive
  replay callbacks, and user-facing recovery reports.
- Exhaustive process-kill matrix in normal CI. The localfs crash harness remains
  bounded and feature-gated.
- Shared lifecycle fuzz scaffold targets. Each lifecycle fuzz target now names
  and calls its own contract.

### Deferred By Owner Slice

- Final closeout inventory, sensitivity-ledger consolidation, and command-matrix
  enforcement: L8P.
- Nightly/libfuzzer execution in CI remains optional; normal tests verify target
  registration, distinct routing, and seed corpora.
- Distributed object-store lease/crash fault simulation remains later durable
  backend work.

### Tests Added

- `lifecycle_property_harness_runs_generated_script_contract`
- `lifecycle_property_harness_requires_input_derived_recovery_routes`
- `lifecycle_property_harness_requires_input_derived_maintenance_routes`
- `lifecycle_property_harness_requires_input_derived_retention_routes`
- `lifecycle_property_harness_requires_input_derived_quarantine_routes`
- `lifecycle_property_harness_requires_input_derived_close_routes`
- `lifecycle_property_harness_replays_minimized_failure_case`
- `lifecycle_property_harness_records_regression_file`
- `lifecycle_generated_script_exercises_input_derived_open_recovery_and_close`
- `lifecycle_generated_script_exercises_input_derived_maintenance_routes`
- `lifecycle_generated_script_exercises_input_derived_reclaim_routes`
- `lifecycle_generated_script_rejects_validation_only_script_without_side_effect_claim`
- `lifecycle_generated_script_model_matches_healthy_recovered_visibility`
- `lifecycle_generated_script_deletion_set_is_subset_of_model_proof`
- `lifecycle_generated_script_watermarks_are_monotonic`
- `lifecycle_generated_script_close_is_idempotent_after_success`
- `lifecycle_generated_script_cache_mode_never_claims_durable_recovery`
- `lifecycle_generated_script_lossy_recovery_records_degraded_health`
- `lifecycle_generated_integration_runs_default_mode_script`
- `lifecycle_generated_integration_runs_durable_mode_script`
- `lifecycle_generated_integration_runs_reclaim_close_script`
- `lifecycle_fault_integration_covers_all_phase_families`
- `lifecycle_crash_integration_reports_case_counts`
- `generated_recovery_empty_checkpoint_tail_and_lossy_routes_are_input_driven`
- `generated_recovery_corrupt_manifest_snapshot_wal_and_table_are_typed`
- `generated_bootstrap_catches_allocator_timestamp_and_visible_facts`
- `generated_bootstrap_rejects_timeline_mismatch`
- `generated_bootstrap_reconciles_unresolved_durable_gate`
- `generated_recovery_health_matches_fault_family_model`
- `fault_capability_mismatch_happens_before_durable_side_effects`
- `fault_writer_guard_acquired_then_manifest_create_fails_releases_or_reports_guard`
- `fault_manifest_create_visible_but_publish_uncertain_records_health_debt`
- `fault_snapshot_published_manifest_update_fails_records_orphan_snapshot`
- `fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success`
- `fault_partial_wal_tail_strict_fails_before_repair`
- `fault_partial_wal_tail_lossy_repairs_and_degrades_health`
- `fault_corrupt_wal_returns_typed_recovery_error`
- `fault_replay_failure_transitions_bootstrap_to_failed`
- `fault_replay_visible_publication_failure_records_durable_not_visible`
- `fault_flush_table_published_branch_install_fails_reports_orphan_table`
- `fault_table_rewrite_branch_swap_failure_preserves_reads`
- `fault_incomplete_retention_proof_blocks_delete_before_backend_access`
- `fault_quarantine_inventory_publish_failure_blocks_purge`
- `fault_purge_delete_success_inventory_update_failure_preserves_debt`
- `fault_close_quiesce_timeout_is_retryable`
- `fault_close_wal_sync_failure_preserves_source_chain`
- `fault_close_manifest_sync_failure_preserves_final_fact_debt`
- `fault_writer_guard_missing_at_release_is_typed`
- `crash_after_wal_append_before_visibility_replays_record`
- `crash_after_wal_append_with_unresolved_gate_reconciles_on_reopen`
- `crash_after_snapshot_publish_before_manifest_update_ignores_orphan_snapshot`
- `crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail`
- `crash_after_table_publish_before_branch_install_reports_orphan_table`
- `crash_after_quarantine_inventory_publish_before_object_move_reports_debt`
- `crash_after_object_quarantine_before_purge_preserves_quarantine_entry`
- `crash_after_close_wal_sync_before_guard_release_reopens_consistently`
- `crash_harness_ignored_cases_have_nonignored_phase_equivalents`
- `crash_harness_respects_case_limit_and_keep_root_environment`
- `lifecycle_fuzz_targets_are_registered`
- `lifecycle_fuzz_targets_call_distinct_contracts`
- `lifecycle_fuzz_corpora_have_non_empty_seed_files`
- `lifecycle_recovery_fuzz_seed_hits_valid_and_corrupt_routes`
- `lifecycle_maintenance_fuzz_seed_hits_task_and_close_routes`
- `lifecycle_retention_fuzz_seed_hits_delete_and_defer_routes`
- `lifecycle_generated_assurance_stays_in_testkit_tests_or_fuzz`
- `lifecycle_production_does_not_import_testkit_or_fuzz`
- `lifecycle_fuzz_targets_use_distinct_contracts`
- `lifecycle_fuzz_corpora_are_seeded`
- `lifecycle_crash_tests_are_feature_gated`
- `ignored_crash_tests_have_nonignored_phase_equivalents`
- `lifecycle_generated_properties_assert_input_derived_counters`
- `lifecycle_assurance_tests_avoid_sleeps_and_thread_spawns`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Generated prelude masks input | `crates/storage-next/src/testkit/lifecycle/script.rs` | Remove input-derived route checks | `lifecycle_property_harness_requires_input_derived_recovery_routes` / generated property harness |
| Recovery health collapse | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Do not require strict/lossy recovery fault routes | `fault_corrupt_wal_returns_typed_recovery_error` |
| Unsafe retention | `crates/storage-next/src/testkit/lifecycle/script.rs` | Skip deletion subset check | `generated_retention_never_deletes_reachable_tables_or_live_snapshots` |
| Stale purge proof | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Do not require stale purge route | `fault_quarantine_inventory_publish_failure_blocks_purge` |
| Checkpoint truncation too aggressive | `crates/storage-next/src/testkit/lifecycle/script.rs` | Drop watermark monotonic check | `generated_checkpoint_truncation_never_removes_uncovered_wal_records` |
| Close starts ordinary work | `crates/storage-next/src/testkit/lifecycle/fault.rs` | Skip close quiesce/timeout route | `generated_close_blocks_new_commits_and_ordinary_maintenance` |
| Fuzz target shares scaffold | `crates/storage-next/fuzz/fuzz_targets/lifecycle_recovery.rs` | Call scaffold contract instead of recovery fuzz contract | `lifecycle_fuzz_targets_call_distinct_contracts` |
| Empty corpora | `crates/storage-next/fuzz/corpus/lifecycle_recovery/valid_seed` | Empty or remove seed file | `lifecycle_fuzz_corpora_have_non_empty_seed_files` |
| Crash test not gated | `crates/storage-next/tests/crash_recovery.rs` | Remove localfs/testkit/wasm cfg from crash routes | `lifecycle_crash_tests_are_feature_gated` |
| Production imports testkit | `crates/storage-next/src/lifecycle/*.rs` | Import testkit or fuzz helpers from production lifecycle source | `lifecycle_production_does_not_import_testkit_or_fuzz` |

### Verification

Commands run for L8O:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --bins
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8P - Lifecycle Conformance Closeout

Status: implemented

### Shipped Files

- `crates/storage-next/tests/lifecycle_closeout.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8p-lifecycle-conformance-closeout-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

### Preserved As Storage Vocabulary

- Closeout remains storage-internal and implementation-focused: lifecycle
  state, storage mode, recovery health, maintenance outcomes, retention proof,
  quarantine facts, close facts, fuzz contracts, crash/reopen counters, and
  source-boundary facts.
- L8 remains crate-private. L9 remains the future public lifecycle/open/close
  API boundary.
- Closeout checks source and test artifacts, not planning-document links.

### Intentional Changes

- Added `lifecycle_closeout.rs` to assert implementation inventory across
  generated/property, fault, crash, fuzz, source-guard, and integration
  surfaces.
- Extended source-guard coverage so closeout and fuzz-inventory assurance files
  are included in deterministic-test checks.
- Recorded a final sensitivity ledger for lifecycle-specific closeout probes.
- Recorded explicit deferrals for non-L8 behavior.

### Retired From V1 L8P

- Public lifecycle APIs and public maintenance commands.
- Product recovery/open/close wording.
- Engine primitive reconstruction callbacks.
- Engine observer callbacks and product background worker policy.
- Distributed object-store lease/fencing race simulation.
- StrataHub behavior.
- Query, index, search, graph, vector, embedding, or inference side effects.

### Deferred By Owner Slice

- Public lifecycle/open/close wrappers: L9.
- Product recovery wording and product open policy: engine-next/L9.
- Primitive reconstruction callbacks: engine-next.
- Background worker thread scheduling: engine-next/post-V1.
- Exhaustive process-kill matrix in default CI: post-V1 optional assurance.
- Distributed object-store lease/fencing races: object backend work.
- StrataHub sync/push/pull behavior: StrataHub integration layers.
- Query/index/search side effects: later query/index layers.
- Memory-budget tuning and Raspberry Pi Zero budget review: later architecture
  review after M4 completion.

### Tests Added

- `lifecycle_closeout_generated_counters_cover_required_categories`
- `lifecycle_closeout_fault_windows_cover_required_phases`
- `lifecycle_closeout_crash_windows_cover_required_phases`
- `lifecycle_closeout_fuzz_targets_and_corpora_are_distinct`
- `lifecycle_closeout_source_guards_cover_required_boundaries`
- `lifecycle_closeout_integration_surfaces_cover_required_categories`
- `lifecycle_closeout_has_no_mutation_probe_artifacts`

### Sensitivity Probes Recorded

| Probe | Mutated file/function | Mutation | Evidence | Verification | Status |
|---|---|---|---|---|---|
| S1 | `src/lifecycle/cache.rs` cache open/outcome path | Cache mode reports durable recovery facts | `storage_open_outcome_rejects_cache_durable_recovery_claims`, `lifecycle_generated_script_cache_mode_never_claims_durable_recovery` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties` | Covered-by-test |
| S2 | `src/lifecycle/durable.rs` open assembly | Durable open creates objects before capability validation | `durable_capability_rejection_happens_before_writer_lock`, `fault_capability_mismatch_happens_before_durable_side_effects` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults` | Covered-by-test |
| S3 | `src/lifecycle/durable/bootstrap.rs` bootstrap failure path | Bootstrap replay failure leaves state recovering | `bootstrap_replay_rejects_mismatched_unresolved_durable_gate`, `fault_replay_failure_transitions_bootstrap_to_failed` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults` | Covered-by-test |
| S4 | `src/lifecycle/durable/bootstrap.rs` visible catch-up | Recovered visible version advances beyond trusted facts | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator`, `generated_bootstrap_catches_allocator_timestamp_and_visible_facts` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | Covered-by-test |
| S5 | `src/lifecycle/recovery.rs` WAL tail repair | Strict recovery repairs partial tail instead of failing | `recovery_rejects_latest_partial_log_tail_in_strict_mode`, `fault_partial_wal_tail_strict_fails_before_repair` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults` | Covered-by-test |
| S6 | `src/lifecycle/recovery.rs` lossy snapshot handling | Missing snapshot in lossy mode reports healthy | `lossy_missing_snapshot_allows_uncertain_flush_watermark_as_degraded_data_loss`, `generated_recovery_health_matches_fault_family_model` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | Covered-by-test |
| S7 | `src/lifecycle/flush.rs` flush path | Flush persists watermark or truncates WAL directly | `durable_flush_does_not_persist_watermark_or_truncate_log`, `lifecycle_flush_source_does_not_manage_watermarks_or_log_retention` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | Covered-by-test |
| S8 | `src/lifecycle/recovery.rs` checkpoint decoder | Recovery rejects opaque snapshot sections | `checkpoint_recovery_ignores_opaque_snapshot_sections` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests` | Covered-by-test |
| S9 | `src/lifecycle/checkpoint.rs` WAL truncation | Truncates records above proven watermark | `generated_checkpoint_truncation_never_removes_uncovered_wal_records`, `crash_after_manifest_update_before_wal_truncation_recovers_checkpoint_and_tail` | `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance`; `cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery` | Covered-by-test |
| S10 | `src/lifecycle/compaction.rs` materialization task binding | Queued materialization uses stale naked layer index | `queued_materialization_uses_bound_source_after_layer_reindex`, `lifecycle_closeout_integration_surfaces_cover_required_categories` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | Covered-by-test |
| S11 | `src/lifecycle/retention.rs` table retention | Deletes reachable table object | `reachable_table_object_is_retained`, `generated_retention_never_deletes_reachable_tables_or_live_snapshots` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | Covered-by-test |
| S12 | `src/lifecycle/retention.rs` snapshot pruning | Deletes live manifest snapshot | `snapshot_pruning_retains_live_manifest_snapshot`, `generated_snapshot_pruning_retains_live_and_newest_snapshots` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | Covered-by-test |
| S13 | `src/lifecycle/quarantine.rs` purge proof | Treats stale purge proof as fresh | `purge_requires_fresh_proof_before_backend_access`, `generated_purge_requires_fresh_inventory_proof` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | Covered-by-test |
| S14 | `src/lifecycle/quarantine.rs` repair | Repair mutates branch or invents missing object | `repair_mutation_is_rejected_before_backend_access`, `generated_repair_reports_inconclusive_without_mutating_state` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | Covered-by-test |
| S15 | `src/lifecycle/durable/close.rs` close admission | Close starts ordinary maintenance after close requested | `close_requested_blocks_ordinary_maintenance`, `generated_close_blocks_new_commits_and_ordinary_maintenance` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | Covered-by-test |
| S16 | `src/lifecycle/durable/close.rs` guard release | Releases writer guard before sync failure is resolved | `durable_close_log_sync_failure_preserves_writer_guard_for_retry`, `fault_close_wal_sync_failure_preserves_source_chain` | `cargo test -p strata-storage-next --locked --lib lifecycle::tests`; `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults` | Covered-by-test |
| S17 | `fuzz/fuzz_targets/lifecycle_*.rs` fuzz routing | All lifecycle fuzz targets call shared scaffold | `lifecycle_fuzz_targets_call_distinct_contracts`, `lifecycle_fuzz_targets_use_distinct_contracts` | `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory`; `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | Covered-by-test |
| S18 | `tests/crash_recovery.rs` crash cfg | Removes localfs/testkit/wasm gating | `lifecycle_crash_tests_are_feature_gated` | `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | Covered-by-test |
| S19 | `src/lifecycle/*.rs` production imports | Imports testkit or fuzz from production lifecycle source | `lifecycle_production_does_not_import_testkit_or_fuzz` | `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | Covered-by-test |
| S20 | `src/lifecycle/*.rs` product imports | Imports engine/product/StrataHub modules | `lifecycle_source_does_not_import_engine_product_or_raw_io` | `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | Covered-by-test |

### Verification

Commands run for L8P:

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test lifecycle_closeout --test lifecycle_source_guard --test lifecycle_fuzz_inventory
cargo test -p strata-storage-next --no-default-features --locked lifecycle
cargo check -p strata-storage-next --no-default-features --locked --tests
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked
cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins
cargo hack check -p strata-storage-next --feature-powerset --depth 2
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

All commands in the closeout matrix were run and passed:

| Command | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --locked --lib lifecycle::tests` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | PASS |
| `cargo test -p strata-storage-next --features fault-injection,testkit --locked --test lifecycle_faults` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_fuzz_inventory` | PASS |
| `cargo test -p strata-storage-next --features localfs,testkit --locked --test crash_recovery` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS |
| `cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard` | PASS |
| `cargo test -p strata-storage-next --all-features --locked --test lifecycle_closeout --test lifecycle_source_guard --test lifecycle_fuzz_inventory` | PASS |
| `cargo test -p strata-storage-next --no-default-features --locked lifecycle` | PASS |
| `cargo check -p strata-storage-next --no-default-features --locked --tests` | PASS |
| `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked` | PASS |
| `cargo check --manifest-path crates/storage-next/fuzz/Cargo.toml --locked --bins` | PASS |
| `cargo hack check -p strata-storage-next --feature-powerset --depth 2` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

Optional nightly/libfuzzer smoke runs were executed after closeout. The
maintenance target initially minimized a seed that let the generated
materialization helper reuse the same branch id for parent and child; the
testkit branch-id derivation now forces distinct ids and
`lifecycle_maintenance_fuzz_regression_keeps_materialization_branches_distinct`
pins the minimized input.

```bash
cargo +nightly fuzz run lifecycle_recovery -- -max_total_time=60
cargo +nightly fuzz run lifecycle_maintenance -- -max_total_time=60
cargo +nightly fuzz run lifecycle_retention -- -max_total_time=60
```

## L8M - Quarantine, Reclaim, Purge, And Repair

### Shipped Files

- `crates/storage-next/src/lifecycle/quarantine.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/tests/quarantine.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/quarantine.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8m-quarantine-reclaim-repair-test-plan.md`

### Preserved As Storage Vocabulary

- Quarantine proofs distinguish safe, referenced, incomplete, and
  recovery-health-blocked states.
- Quarantine operation outcomes record source object, quarantine object,
  inventory object, byte count, entry count, recovery health, retryability, and
  lower-layer source errors.
- Purge proofs distinguish fresh, stale, incomplete, and
  recovery-health-blocked states before any backend mutation can happen.
- Purge outcomes report deleted, already-missing, failed, and retained
  quarantine entries plus reclaimed byte counts from inventory facts.
- Repair outcomes report branch or family reconciliation facts for listed,
  missing, unlisted, malformed, and inventory-present states.

### Raw Health And Fact Vocabulary

- Quarantine publication uncertainty and publication failure have stable
  lifecycle error codes.
- Inventory mismatch and repair inconclusive states preserve lower-layer service
  errors when available.
- Quarantine and purge deferred outcomes carry telemetry health debt rather than
  claiming durable reclaim success.
- Maintenance outcomes preserve affected object names, state-change counts,
  reclaimed bytes when known, source errors, and recovery-health debt.

### Intentional Changes

- Lifecycle quarantine delegates all durable copy, inventory publication,
  source deletion, purge, and repair operations to `QuarantineService`.
- Cache mode has no durable quarantine mutation surface.
- Retention remains proof-only; it delegates quarantine object families rather
  than mutating inventory or deleting objects directly.
- Durable maintenance now has concrete quarantine purge and repair runners for
  branch and family scopes.
- Non-publication service failures are classified separately from quarantine
  object publication failures so missing source objects and malformed requests
  are not advertised as retryable publish windows.

### Retired From V1 L8M

- Direct backend deletion from lifecycle quarantine code.
- Lifecycle-owned quarantine inventory encoding/decoding.
- Product repair reports.
- Runtime-only reachability proofs as sufficient evidence for destructive
  reclaim.

### Deferred By Owner Slice

- Close-time final quarantine drain: L8N.
- Crash/fuzz assurance expansion: L8O/L8P.
- Public repair and purge commands: L9.
- Automatic table-manifest-backed quarantine proof assembly: later durable
  table-manifest work.

### Tests Added

- `quarantine_proof_complete_from_candidate_and_blocks_unsafe_health`
- `quarantine_incomplete_proof_defers_without_backend_access`
- `quarantine_stages_inventory_copy_and_source_delete_in_order`
- `quarantine_source_delete_failure_reports_retryable_health_debt`
- `quarantine_missing_source_is_transient_service_failure_not_publish_failure`
- `quarantine_proof_allows_unrelated_telemetry_debt`
- `purge_request_rejects_missing_database_id_before_backend_access`
- `purge_requires_fresh_proof_before_backend_access`
- `purge_deletes_inventory_listed_quarantine_objects`
- `repair_request_rejects_missing_database_id_before_backend_access`
- `repair_reports_unlisted_quarantine_object_as_health_debt`
- `durable_quarantine_runs_through_runtime_maintenance_surface`
- `durable_purge_runs_through_runtime_maintenance_surface`
- `durable_repair_runs_through_runtime_maintenance_surface`
- `purge_and_repair_maintenance_requests_preserve_branch_scope`
- `quarantine_errors_have_stable_codes`
- `lifecycle_quarantine_integration`
- `lifecycle_purge_integration`
- `lifecycle_repair_reconciliation_integration`
- `lifecycle_reclaim_blocks_unsafe_recovery_integration`
- `lifecycle_cache_reclaim_unsupported_integration`
- `lifecycle_quarantine_then_purge_round_trip`
- `lifecycle_quarantine_publish_failure_surfaces_health_debt`
- `lifecycle_quarantine_generated_bytes_influence_routes`
- `lifecycle_quarantine_source_uses_quarantine_service_boundary`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Unsafe health quarantines | `crates/storage-next/src/lifecycle/quarantine.rs` | Treat blocked recovery health as complete proof | `quarantine_proof_complete_from_candidate_and_blocks_unsafe_health` |
| Incomplete proof mutates backend | `crates/storage-next/src/lifecycle/quarantine.rs` | Call quarantine service for incomplete proof | `quarantine_incomplete_proof_defers_without_backend_access` |
| Source delete before durable copy | `crates/storage-next/src/service/quarantine/mutation.rs` | Reorder source delete before inventory/copy | `quarantine_stages_inventory_copy_and_source_delete_in_order` |
| Delete failure hidden | `crates/storage-next/src/lifecycle/quarantine.rs` | Collapse source delete error to completed outcome | `quarantine_source_delete_failure_reports_retryable_health_debt` |
| Missing source misclassified | `crates/storage-next/src/lifecycle/quarantine.rs` | Report source metadata/read failures as publish failures | `quarantine_missing_source_is_transient_service_failure_not_publish_failure` |
| Stale purge proof deletes | `crates/storage-next/src/lifecycle/quarantine.rs` | Treat stale purge proof as fresh | `purge_requires_fresh_proof_before_backend_access` |
| Purge deletes unlisted object | `crates/storage-next/src/service/quarantine/mutation.rs` | Delete by prefix instead of inventory entries | `purge_deletes_inventory_listed_quarantine_objects` |
| Purge drops byte facts | `crates/storage-next/src/service/quarantine/mutation.rs` | Do not accumulate reclaimed bytes from deleted inventory entries | `purge_deletes_inventory_listed_quarantine_objects` |
| Repair hides unlisted object | `crates/storage-next/src/lifecycle/quarantine.rs` | Ignore unlisted reconciliation facts | `repair_reports_unlisted_quarantine_object_as_health_debt` |
| Runtime runner bypass | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Remove purge or repair runtime maintenance runner wiring | `durable_purge_runs_through_runtime_maintenance_surface` / `durable_repair_runs_through_runtime_maintenance_surface` |
| Branch purge scope rejected | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove branch scope support for purge tasks | `purge_and_repair_maintenance_requests_preserve_branch_scope` |
| Error code collapses | `crates/storage-next/src/lifecycle/error.rs` | Route quarantine failures through generic maintenance code | `quarantine_errors_have_stable_codes` |
| Lifecycle bypasses service boundary | `crates/storage-next/src/lifecycle/quarantine.rs` | Call backend delete or encode inventory directly | `lifecycle_quarantine_source_uses_quarantine_service_boundary` |

### Verification

Commands run for L8M:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib service::quarantine
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8K: Compaction And Materialization Scheduling Hooks

### Shipped Files

- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/compaction.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8k-compaction-materialization-scheduling-test-plan.md`

### Preserved As Storage Vocabulary

- Compaction is requested with a branch id, branch compaction kind, output
  identity seed, and table-rewrite durability mode.
- Materialization is requested with a child branch id, inherited-layer index,
  output identity prefix, and table-rewrite durability mode.
- Outcomes report completed, completed-checkpoint-required, no-candidate,
  no-layer, and already-materialized states.
- Storage pressure facts report frozen backlog, level-zero table backlog,
  inherited-layer backlog, maintenance queue backlog, and suggested storage
  maintenance tasks.

### Raw Health And Fact Vocabulary

- V1 durable-local compaction/materialization reports checkpoint debt instead
  of standalone table-object reachability.
- Lower-layer branch runtime errors preserve source chains through
  `LifecycleLowerLayer::BranchRuntime`.
- Maintenance outcomes retain task kind, status, task id, affected-object
  count, stats, retryability, and optional recovery health.

### Intentional Changes

- Lifecycle delegates all candidate selection, table replacement validation,
  inherited-row rewriting, and read semantics to L6. It does not inspect rows
  to choose merge inputs.
- Cache and durable runtimes share the same L6 rewrite paths. Durable mode
  upgrades successful rewrites to checkpoint-required outcomes.
- The source guard now permits storage level names such as `CompactL0...` while
  continuing to reject architecture slice labels like `L8K` in implementation
  source and tests.

### Deferred By Owner Slice

- Standalone table-object publication for compaction/materialization waits for
  table-manifest recovery, so published-not-installed fault windows are not
  claimed here.
- Generated lifecycle property scripts for table rewrites remain later
  assurance-depth work.
- Retention pruning, replaced-object quarantine/purge, background thread
  scheduling, and memory-budget admission remain later slices.

### Tests Added

- `table_rewrite_requests_validate_opaque_identity_components`
- `maintenance_tasks_map_to_table_rewrite_requests`
- `cache_compaction_defers_without_a_candidate`
- `cache_compaction_installs_replacement_and_preserves_reads`
- `durable_compaction_reports_checkpoint_debt`
- `materialization_defers_when_no_inherited_layer_exists`
- `cache_materialization_removes_layer_and_preserves_child_precedence`
- `durable_materialization_reports_checkpoint_debt`
- `storage_pressure_suggests_the_next_table_rewrite_or_flush`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Empty output seed accepted | `crates/storage-next/src/lifecycle/compaction.rs` | Remove lower-layer request validation | `table_rewrite_requests_validate_opaque_identity_components` |
| Wrong maintenance kind routed as compaction | `crates/storage-next/src/lifecycle/compaction.rs` | Skip task-kind check before request conversion | `maintenance_tasks_map_to_table_rewrite_requests` |
| No-candidate treated as success | `crates/storage-next/src/lifecycle/compaction.rs` | Collapse no-candidate to completed outcome | `cache_compaction_defers_without_a_candidate` |
| Durable rewrite overclaims recovery safety | `crates/storage-next/src/lifecycle/compaction.rs` | Return completed instead of checkpoint-required status | `durable_compaction_reports_checkpoint_debt` |
| Materialization ignores child-local precedence | `crates/storage-next/src/branch/state.rs` | Install replacements ahead of child-owned rows | `cache_materialization_removes_layer_and_preserves_child_precedence` |
| Missing inherited layer becomes hard failure | `crates/storage-next/src/lifecycle/compaction.rs` | Return branch error instead of deferred no-layer outcome | `materialization_defers_when_no_inherited_layer_exists` |
| Frozen backlog deprioritized behind compaction | `crates/storage-next/src/lifecycle/compaction.rs` | Suggest compaction before flush when frozen tables exist | `storage_pressure_suggests_the_next_table_rewrite_or_flush` |
| Architecture label allowed in lifecycle source | `crates/storage-next/tests/lifecycle_source_guard.rs` | Permit `L8K`-style labels in implementation text | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8K:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8I - Flush Frozen State And Table Publication

Status: implemented.

### Shipped Files

- `crates/storage-next/src/lifecycle/flush.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/flush.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/flush.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8i-flush-table-publication-test-plan.md`

### What Landed

- Added a concrete flush request/outcome surface for one frozen branch table at
  a time.
- Added deterministic frozen-table selection: explicit index when supplied,
  otherwise the oldest frozen table.
- Added cache-mode flush orchestration that builds an immutable table from
  frozen rows and installs it back into branch state without durable object
  services.
- Added durable-local flush orchestration that publishes a table object,
  reopens the published object through the table reader service, constructs a
  branch-owned immutable table, and only then replaces frozen state.
- Added retry handling for the already-created-object window by reopening the
  matching deterministic table object instead of republishing it.
- Uses full SHA-256 hex in deterministic table/object identities rather than a
  short fingerprint, so retry identity is stable without accepting avoidable
  alias risk.
- Durable outcomes count affected objects only when a table object exists;
  cache-mode flushes report zero durable object effects.
- Added maintenance task request construction for branch-scoped flush tasks.
- Added concrete cache and durable runtime dispatch for queued branch-scoped
  flush tasks through the maintenance executor.
- Added generated flush contract coverage under the lifecycle testkit for
  cache success, durable success, no-op, publish failure, reopen failure, retry,
  and read parity.
- Added a branch-layer storage-vocabulary alias for frozen replacement so
  lifecycle implementation code does not contain numbered layer labels.

### Preserved As Storage Vocabulary

- Request facts: branch id, optional frozen index, table identity seed, table
  object id, and target branch level.
- Outcome facts: status, branch id, replaced frozen index, row count, table
  identity, table facts, table object, object facts, install outcome, and
  failure source when a durable object was created but branch install did not
  complete.
- Maintenance mapping: completed, deferred, and retryable failed outcomes map
  onto the generic maintenance outcome vocabulary.

### Intentional Non-Goals

- No database manifest flush watermark update.
- No checkpoint publication.
- No WAL retention or truncation.
- No compaction, materialization scheduling, retention, quarantine, purge, or
  repair.
- No public maintenance command surface.

### Tests Added

- `flush_request_validates_components_and_target_level`
- `flush_without_frozen_state_is_deferred`
- `cache_flush_replaces_oldest_frozen_table_and_preserves_reads`
- `cache_flush_replaces_named_table_and_keeps_other_frozen_order`
- `cache_flush_preserves_tombstones_and_commit_timestamps`
- `repeated_default_flush_after_success_is_deferred`
- `cache_runtime_flushes_explicitly_rotated_state_only`
- `queued_cache_flush_task_runs_through_executor`
- `durable_flush_publishes_reopens_and_installs_table`
- `queued_durable_flush_task_publishes_object_through_executor`
- `durable_publish_failure_leaves_frozen_state_unchanged`
- `durable_reopen_failure_reports_published_not_installed`
- `durable_invalid_publish_metadata_preserves_service_source`
- `durable_reopen_wrong_branch_table_reports_partial_publication`
- `durable_install_failure_reports_orphaned_object_fact`
- `existing_conflicting_object_fails_closed_without_removing_frozen_rows`
- `durable_flush_retries_existing_matching_object`
- `flush_named_frozen_index_must_exist`
- `flush_identity_is_deterministic_and_changes_with_storage_facts`
- `lifecycle_maintenance_contract_covers_flush_categories`
- `lifecycle_property_harness_runs_flush_contract`
- `lifecycle_flush_source_does_not_manage_watermarks_or_log_retention`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Select newest frozen table by default | `crates/storage-next/src/lifecycle/flush.rs` | Return index 0 instead of the highest frozen index | `cache_flush_replaces_oldest_frozen_table_and_preserves_reads` |
| Remove frozen state before durable publication | `crates/storage-next/src/lifecycle/flush.rs` | Call branch replacement before table-object publish | `durable_publish_failure_leaves_frozen_state_unchanged` |
| Skip object reopen after durable publish | `crates/storage-next/src/lifecycle/flush.rs` | Build branch-owned table directly from in-memory bytes after publish | `durable_flush_publishes_reopens_and_installs_table` |
| Treat active rows as an implicit flush candidate | `crates/storage-next/src/lifecycle/cache.rs` | Rotate active rows inside flush | `cache_runtime_flushes_explicitly_rotated_state_only` |
| Lose retry support for existing deterministic object | `crates/storage-next/src/lifecycle/flush.rs` | Return publish precondition failure directly | `durable_flush_retries_existing_matching_object` |
| Collapse cache and durable object effects | `crates/storage-next/src/lifecycle/flush.rs` | Count table identity instead of table object in maintenance effects | `cache_flush_replaces_oldest_frozen_table_and_preserves_reads`, `durable_flush_publishes_reopens_and_installs_table` |
| Shorten deterministic digest | `crates/storage-next/src/lifecycle/flush.rs` | Truncate SHA-256 to a short prefix | `flush_identity_is_deterministic_and_changes_with_storage_facts` |
| Treat published-not-installed as success | `crates/storage-next/src/lifecycle/flush.rs` | Return completed after object publish but before reopen/install succeeds | `durable_reopen_failure_reports_published_not_installed`, `durable_install_failure_reports_orphaned_object_fact` |
| Drop tombstones during table build | `crates/storage-next/src/lifecycle/flush.rs` | Filter tombstone rows from the built table | `cache_flush_preserves_tombstones_and_commit_timestamps` |
| Accept conflicting existing object | `crates/storage-next/src/lifecycle/flush.rs` | Treat any pre-existing deterministic object as matching | `existing_conflicting_object_fails_closed_without_removing_frozen_rows` |
| Call watermark or log-retention services from flush | `crates/storage-next/src/lifecycle/flush.rs` | Add manifest flush-watermark or log truncation calls | `lifecycle_flush_source_does_not_manage_watermarks_or_log_retention` |
| Put architecture labels in lifecycle implementation | `crates/storage-next/src/lifecycle/flush.rs` | Call numbered lower-layer method names directly | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8I:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8B - Lifecycle State And Open Plan

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8b-lifecycle-state-open-plan-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/lifecycle/tests/state.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`

### Preserved As Storage Vocabulary

- Side-effect-free lifecycle state transitions for new, opening, recovering,
  open, closing, closed, and failed.
- Transition triggers for open requested, cache ready, durable recovery needed,
  recovery accepted, close requested, close completed, close retried, and phase
  failure.
- Operation admission facts for open, ordinary read, commit, recovery step,
  ordinary maintenance, close-required drain, health query, close, and close
  retry.
- Failure facts that preserve the failed storage phase and reason.
- Close facts that distinguish requested, retry-pending, complete, and
  already-closed idempotence.
- Storage open disposition facts for created vs opened-existing outcomes.

### Intentional Changes

- `StorageOpenOutcome` now stores `StorageOpenDisposition` instead of a raw
  boolean while keeping the derived `opened_existing()` getter.
- Cache-mode open outcomes reject durable recovery degradation as well as
  recovered durable visible versions.
- State transition validation is centralized in `lifecycle/state.rs`; invalid
  transitions return `LifecycleError::InvalidLifecycleState` without mutating
  machine state.
- Closed close retry is the only idempotent state transition in L8B.
- Closed close and closed close retry are explicitly admitted as idempotent
  operations; closing close retry remains retryable but not complete.
- Direct lifecycle tests were split into `src/lifecycle/tests/mod.rs` and
  `src/lifecycle/tests/state.rs` before the file grew past the local
  maintainability threshold.

### Retired From V1 L8B

- Raw public open policy booleans in storage open outcome facts.
- Any product API, engine handle, StrataHub, follower, or public maintenance
  vocabulary in lifecycle state/admission code.
- Any backend, service, WAL, manifest, snapshot, branch, commit, maintenance, or
  close side effects in the L8B state layer.

### Deferred By Owner Slice

- Backend and service capability validation: L8C.
- Cache-mode runtime open and close baseline: L8D.
- Durable service assembly: L8E.
- Recovery orchestration, WAL replay, and L7 replay/bootstrap: L8F-L8G.
- Maintenance executor and task queue execution: L8H.
- Close drain, durable sync, and guard release side effects: L8N.
- Cross-slice fault, crash, fuzz, and closeout inventory: L8O-L8P.

### Tests Added

- `lifecycle_state_machine_initial_state_admits_only_open_and_health`
- `lifecycle_state_machine_accepts_open_and_recovery_transitions`
- `lifecycle_state_machine_accepts_close_and_retry_transitions`
- `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state`
- `lifecycle_operation_admission_matrix_is_state_specific`
- `lifecycle_failure_facts_preserve_phase_and_reject_empty_reasons`
- `lifecycle_close_retry_and_closed_idempotence_are_distinct`
- Open-outcome validation coverage for cache durable recovery degradation.
- Generated lifecycle scaffold counters for valid transitions, invalid
  transitions, admission accepts, admission rejects, close retry,
  closed-idempotence, failed-state stickiness, and input-derived state routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Transition skip | Allow `New + CacheOpenReady -> Open` | `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state` |
| Recovery exposure | Allow ordinary read in `Recovering` | `lifecycle_operation_admission_matrix_is_state_specific` |
| Commit outside open | Allow commit in `Opening` or `Closing` | `lifecycle_operation_admission_matrix_is_state_specific` |
| Close false success | Treat `Closing + CloseRetried` as `Closed` | `lifecycle_close_retry_and_closed_idempotence_are_distinct` |
| Failed-state loosened | Allow open or close retry in `Failed` | `lifecycle_state_machine_rejects_undocumented_transitions_without_mutating_state` |
| Empty failure reason | Accept `PhaseFailed { reason: "" }` | `lifecycle_failure_facts_preserve_phase_and_reject_empty_reasons` |
| Cache degraded recovery | Accept degraded recovery health in cache mode | `storage_open_outcome_rejects_cache_durable_recovery_claims` |

### Verification

Commands to run for L8B:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8C - Storage Mode Capability Validation

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/config/mode.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8c-storage-mode-capability-validation-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/capability.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/capability.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Lifecycle storage modes map to the existing `StorageModeRequest` capability
  checks instead of duplicating backend capability matrices.
- Cache mode accepts browser-like object capabilities without requiring object
  metadata or durable primitives.
- Durable local standard and durable local always share durable backend
  requirements while preserving `DurabilityPolicy::Standard` vs
  `DurabilityPolicy::Always` for later open/runtime wiring.
- Object-durable candidate remains candidate-tagged and accepts either
  `ConditionalPublish` or `ConditionalCreate + ConditionalUpdate` fencing.
- Capability mismatch is a typed lifecycle error carrying the requested storage
  mode, complete required `BackendCapability` list, and exact missing
  capability list.

### Intentional Changes

- Added `validate_storage_mode_capabilities(plan, capabilities)` for pure
  capability-fact validation.
- Added `validate_backend_capabilities_for_open(plan, backend)`, which calls
  only `backend.capabilities()`.
- Added `LifecycleCapabilityOutcome` and `ObjectDurableFenceMode` as
  crate-private lifecycle facts.
- Added display names for lifecycle `StorageMode` so capability errors remain
  bounded and storage-shaped.
- Split the generated lifecycle testkit into `lifecycle/mod.rs`,
  `lifecycle/outcome.rs`, and `lifecycle/capability.rs`.

### Retired From V1 L8C

- Ad hoc lifecycle capability strings.
- Capability validation that constructs services, opens manifests, opens WALs,
  acquires writer locks, or mutates L6/L7 state.
- Product open wording in capability mismatch errors.

### Deferred By Owner Slice

- Cache-mode runtime open and close: L8D.
- Durable service assembly and writer-lock acquisition: L8E.
- Recovery orchestration, WAL replay, and L7 bootstrap: L8F-L8G.
- Maintenance execution, retention, quarantine, repair, and close side effects:
  L8H-L8P.
- Production object-durable mode claims beyond candidate capability validation:
  post-V1 object durability design.

### Tests Added

- `capability_validation_maps_lifecycle_modes_to_storage_mode_requests`
- `cache_capability_validation_accepts_browser_like_backend_without_metadata`
- `durable_local_modes_reject_each_missing_durable_capability`
- `object_candidate_accepts_either_publish_fence_or_create_update_pair`
- `object_candidate_reports_base_and_partial_fence_missing_capabilities`
- `cache_capability_validation_never_requires_durable_storage_capabilities`
- `backend_capability_preflight_reads_only_capabilities`
- `lifecycle_capability_validator_stays_preflight_only`
- Generated lifecycle counters for accepted/rejected capability cases, per-mode
  capability cases, missing-capability categories, object-candidate fence
  variants, backend preflight across every mode, and input-derived capability
  masks.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Cache over-requires metadata | Add `ObjectMetadata` to cache requirements | `cache_capability_validation_accepts_browser_like_backend_without_metadata` |
| Durable under-requires append | Remove `AppendObject` from durable requirements | `durable_local_modes_reject_each_missing_durable_capability` |
| Durable policy collapse | Map durable always to standard policy | `capability_validation_maps_lifecycle_modes_to_storage_mode_requests` |
| Object fence missing | Accept object candidate without any fence | `object_candidate_reports_base_and_partial_fence_missing_capabilities` |
| Fence preference drift | Prefer create/update when conditional publish is also present | `object_candidate_accepts_either_publish_fence_or_create_update_pair` |
| Preflight side effect | Call read/list/write/publish/append/lock during validation | `backend_capability_preflight_reads_only_capabilities` |
| Untyped mismatch | Report only a string reason for capability mismatch | `object_candidate_reports_base_and_partial_fence_missing_capabilities` |

### Verification

Commands run for L8C:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8D - Cache Open And Close

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/commit/cache.rs`
- `crates/storage-next/src/commit/branch_registry.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `crates/storage-next/src/backend/memory.rs`
- `crates/engine/src/database/recovery.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/cache.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/cache.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8d-cache-open-close-test-plan.md`

### Preserved As Storage Vocabulary

- Cache mode opens as volatile storage state only.
- Cache open composes existing L6 `BranchLocalState` and L7
  `CommitCacheRuntime` rather than adding a bespoke row store or commit path.
- Cache open runs L8C backend capability preflight before assembling branch or
  commit runtime state.
- Cache open reports `StorageMode::Cache`, `StorageOpenDisposition::Created`,
  healthy recovery, no recovered durable visible version, and maintenance not
  ready.
- Cache close is a lifecycle state transition and does not perform durable
  shutdown work.

### Intentional Changes

- Added `LifecycleCacheOpenRequest` to carry the cache open plan, initial
  branch id, and branch generation as storage facts.
- Added `LifecycleCacheRuntime<S>` for crate-private volatile cache runtime
  state.
- Added cache commit execution by constructing short-lived `CommitCacheRuntime`
  instances over owned L6/L7 state.
- Added cache read-view access through `BranchLocalState::capture_read_view`.
- Added idempotent cache close using the L8B state machine.
- Extended generated lifecycle counters with cache open, close, commit/read,
  durable-absence, and reopen-empty categories.
- Added a source guard that keeps `lifecycle/cache.rs` out of durable service,
  layout, format, object publication, sync, append, and writer-lock APIs.

### Retired From V1 L8D

- Cache recovery from backend object inventory.
- Cache-mode manifest, WAL, snapshot, table-object, and quarantine service
  construction.
- Cache-mode writer-lock acquisition or release.
- Product open/close, freeze-hook, follower, IPC, primitive, or StrataHub
  behavior.

### Deferred By Owner Slice

- Durable local service assembly and writer-lock acquisition: L8E.
- Durable recovery orchestration: L8F.
- L7 replay/bootstrap from durable facts: L8G.
- Maintenance scheduling, flush, checkpoint, compaction, retention, quarantine,
  repair, and full durable close: later L8 slices.
- Public storage open/read/commit API wrapping: L9.

### Tests Added

- `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims`
- `cache_open_rejects_non_cache_plan_before_backend_preflight`
- `cache_open_request_validation_rejects_invalid_plan_shapes`
- `cache_open_runs_capability_preflight_without_backend_side_effects`
- `cache_runtime_executes_cache_commit_and_reads_through_l6`
- `cache_runtime_generated_timestamp_proves_zero_allocator_and_empty_timestamp_guard`
- `cache_runtime_rejects_wrong_mode_batch_and_preserves_state`
- `cache_runtime_rejects_read_only_wrong_branch_stale_generation_and_conflict`
- `cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls`
- `cache_close_without_commits_completes_and_preserves_diagnostic_facts`
- `cache_reopen_starts_empty_even_when_prior_runtime_committed_rows`
- `lifecycle_cache_runtime_stays_cache_only`
- Generated lifecycle counters for cache open accepted/rejected, cache baseline,
  durable absence, commit/read smoke, close, close idempotence,
  commit-after-close rejection, reopen-empty, and input-derived cache operation
  routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Skip cache capability preflight | Construct runtime before `validate_backend_capabilities_for_open` | `cache_open_runs_capability_preflight_without_backend_side_effects` |
| Backend side effect during cache open | Call `list_prefix`, `read_object`, or writer-lock APIs | `cache_open_runs_capability_preflight_without_backend_side_effects` |
| Cache durable recovery claim | Report recovered visible version or degraded health | `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims` |
| Durable service import | Import WAL/manifest/snapshot/table/quarantine services in `lifecycle/cache.rs` | `lifecycle_cache_runtime_stays_cache_only` |
| Nonzero cache baseline | Start visible tracker above `CommitVersion::ZERO` | `cache_open_builds_volatile_l6_l7_baseline_without_recovery_claims` |
| Persistent cache reopen | Reuse prior volatile branch rows on reopen | `cache_reopen_starts_empty_even_when_prior_runtime_committed_rows` |
| Post-close mutation | Allow commit or ordinary read after close | `cache_close_is_idempotent_blocks_commits_and_reads_and_avoids_backend_calls` |
| Manual cache stamping | Bypass `CommitCacheRuntime` for user rows or timeline rows | `cache_runtime_executes_cache_commit_and_reads_through_l6` |

### Verification

Commands run for L8D:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::cache
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8E - Durable Open/Create Service Assembly

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/capability.rs`
- `crates/storage-next/src/backend/mod.rs`
- `crates/storage-next/src/backend/local_fs.rs`
- `crates/storage-next/src/layout/mod.rs`
- `crates/storage-next/src/format/manifest.rs`
- `crates/storage-next/src/service/manifest.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/sidecar.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/checkpoint.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/branch_registry.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `crates/engine/src/database/open.rs`
- `crates/engine/src/database/recovery.rs`
- `crates/engine/src/database/lifecycle.rs`
- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/durable.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/testkit/lifecycle/durable.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/outcome.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8e-durable-open-create-service-assembly-test-plan.md`

### Preserved As Storage Vocabulary

- Durable local standard and durable local always assemble the same durable L4
  service bundle while preserving `DurabilityPolicy::Standard` vs
  `DurabilityPolicy::Always`.
- Durable assembly runs L8C capability preflight before writer-lock acquisition
  or durable object access.
- Durable assembly acquires the backend writer guard through
  `ObjectLayout::writer_lock()` and owns the guard in the returned shell.
- Missing database manifest creates an initial durable manifest.
- Existing database manifest loads without replacement and preserves snapshot,
  snapshot-id, flush, and active-WAL recovery facts.
- WAL opens on the manifest active segment.
- The returned durable shell stays in `LifecycleState::Recovering`; ordinary
  reads, commits, and ordinary maintenance are not admitted before L8F/L8G
  finish recovery.

### Intentional Changes

- Added `LifecycleDurableLocalOpenRequest` for durable assembly inputs.
- Added `LifecycleDurableAssemblyFacts` for manifest, writer-lock,
  active-WAL, and durability policy facts.
- Added `LifecycleDurableLocalServices<'a>` as the crate-private L4 service
  bundle.
- Added `LifecycleDurableLocalShell<'a, S>` as the recovery-stage shell.
- Made `WalServiceConfig::validate` crate-visible so lifecycle can reject
  invalid WAL config before publishing an initial manifest.
- Extended generated lifecycle counters with durable standard/always assembly,
  durable rejection, manifest create/open, manifest create-race, manifest
  publish-fault, WAL-open failure, lock failure, identity mismatch,
  recovering-state admission, no-recovery side effects, and input-derived
  durable-mode routes.
- Added a source guard that keeps `lifecycle/durable.rs` to assembly work and
  blocks hardcoded writer-lock names, WAL record replay, checkpoint execution,
  and quarantine/recovery calls in this slice.

### Retired From V1 L8E

- Product open policy, registry wiring, primitive reconstruction, IPC, and
  external synchronization behavior.
- Object-durable candidate production open.
- Read-only/follower durable open.
- Background WAL sync thread startup.

### Deferred By Owner Slice

- Snapshot, table, WAL-tail, and quarantine recovery orchestration: L8F.
- L7 replay, allocator catch-up, visible-version restore, timeline validation,
  and final `StorageOpenOutcome`: L8G.
- Maintenance scheduling, flush, checkpoint, WAL truncation, compaction,
  materialization, retention, quarantine mutation, purge, and repair: later L8
  slices.
- Durable close drain, final sync, and explicit writer-guard release: L8N.
- Public open/read/commit wrapping: L9.

### Tests Added

- `durable_assembly_creates_manifest_opens_wal_and_remains_recovering`
- `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts`
- `durable_request_rejects_non_durable_modes_without_backend_calls`
- `durable_request_rejects_codec_mismatch_before_backend_calls`
- `durable_request_rejects_invalid_wal_config_before_backend_calls`
- `durable_capability_rejection_happens_before_writer_lock`
- `durable_writer_lock_failure_happens_before_manifest_access`
- `durable_manifest_identity_mismatch_rejects_before_wal_open`
- `durable_manifest_codec_mismatch_rejects_before_wal_open`
- `durable_manifest_publish_uncertainty_preserves_source_chain`
- `durable_manifest_create_precondition_race_reloads_existing_manifest`
- `durable_manifest_create_precondition_race_reloads_and_revalidates_identity`
- `durable_existing_manifest_decode_failures_reject_before_wal_open`
- `durable_wal_open_failures_are_typed_and_do_not_mark_open`
- `durable_wal_header_database_mismatch_rejects_existing_segment`
- `durable_localfs_writer_lock_excludes_second_shell_until_drop`
- `lifecycle_durable_runtime_stays_assembly_only`
- Generated lifecycle counters for durable standard/always assembly, durable
  rejection, manifest create/open, manifest create-race, manifest
  publish-fault, WAL-open failure, writer-lock failure, manifest identity
  mismatch, recovering-state admission, no-recovery side effects, and
  input-derived durable routes.

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Skip capability preflight | Acquire writer guard before `validate_backend_capabilities_for_open` | `durable_capability_rejection_happens_before_writer_lock` |
| Hardcode writer-lock object | Use `"locks/writer"` literal in lifecycle durable source | `lifecycle_durable_runtime_stays_assembly_only` |
| Manifest before lock | Load manifest before writer guard acquisition | `durable_writer_lock_failure_happens_before_manifest_access` |
| Reject missing manifest | Treat absent manifest as recovery failure | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Replace existing manifest | Publish manifest during existing open | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Ignore database id mismatch | Continue after wrong manifest database id | `durable_manifest_identity_mismatch_rejects_before_wal_open` |
| Ignore codec mismatch | Continue after wrong manifest codec id | `durable_manifest_codec_mismatch_rejects_before_wal_open` |
| Ignore create-race identity | Treat precondition race as opened without reload validation | `durable_manifest_create_precondition_race_reloads_and_revalidates_identity` |
| Accept corrupt manifest | Continue after malformed or future database manifest bytes | `durable_existing_manifest_decode_failures_reject_before_wal_open` |
| Ignore WAL metadata failure | Continue after active WAL segment metadata failure | `durable_wal_open_failures_are_typed_and_do_not_mark_open` |
| Ignore WAL header mismatch | Continue after active WAL segment database id mismatch | `durable_wal_header_database_mismatch_rejects_existing_segment` |
| Open hardcoded WAL segment | Ignore manifest active WAL segment | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Drop writer guard early | Do not retain `BackendWriterGuard` in shell | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Permit second local writer | Allow two durable local shells on the same localfs root | `durable_localfs_writer_lock_excludes_second_shell_until_drop` |
| Mark shell open early | Transition directly to `Open` after service assembly | `durable_assembly_creates_manifest_opens_wal_and_remains_recovering` |
| Collapse always to standard | Pass standard policy for durable always | `durable_assembly_loads_existing_manifest_and_preserves_recovery_facts` |
| Collapse publish uncertainty | Map durability-unconfirmed publish to generic failure | `durable_manifest_publish_uncertainty_preserves_source_chain` |
| Replay during assembly | Construct WAL records or replay runtime in L8E | `lifecycle_durable_runtime_stays_assembly_only` |

### Verification

Commands run for L8E:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::durable
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --all-features --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8F - Recovery Orchestration

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/service/snapshot.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/service/table.rs`
- `crates/storage-next/src/service/quarantine.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/commit/replay.rs`
- `crates/storage-next/src/format/snapshot.rs`
- `crates/storage-next/src/format/storage_row.rs`
- `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8f-recovery-orchestration-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/format/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/recovery.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- L8F starts from the L8E durable shell and requires recovery-step admission.
- Manifest snapshot id/watermark facts are validated before WAL replay-start
  selection.
- Manifest-listed snapshots load through `SnapshotService` with database id,
  codec id, snapshot id, and watermark validation.
- Row-native checkpoint sections use storage row bytes and install through the
  L6 snapshot-install API.
- WAL records are read through `WalService::read_after_commit_version` using
  the trusted recovered checkpoint watermark. A manifest flush watermark that
  is not covered by recovered checkpoint/table state fails closed.
- Latest WAL tail truncation is repaired through `WalService::repair_latest_tail`
  only when the open plan explicitly allows lossy fallback; strict recovery
  rejects partial WAL tails before repair.
- Quarantine inventory loads through `QuarantineService::load_inventory` before
  WAL-tail repair, so a quarantine read/decode failure cannot leave a repaired
  WAL side effect.
- L8F returns a recovery package for L8G and does not invoke L7 replay,
  allocator catch-up, visible-version publication, or product callbacks.

### Intentional Changes

- Added `LifecycleRecoveryRuntime` over `LifecycleDurableLocalShell`.
- Added `LifecycleRecoveryRequest` and crate-private recovery outcome/fact
  structs for checkpoint, WAL, quarantine, and table validation.
- Added `SNAPSHOT_ROW_SECTION_KIND` and `encode_checkpoint_row_section` for
  row-native checkpoint snapshots.
- Re-exported storage-row encode/decode helpers from `format` for lifecycle
  recovery's storage-owned checkpoint section codec.
- Added `MissingSnapshotObject` and `WalTailRepairFailed` recovery fault kinds.
- Missing snapshot and WAL-tail repair degradations classify as `DataLoss`;
  quarantine inventory mismatches classify as `Telemetry`.
- Added mutable shell/service accessors needed by recovery while keeping the
  durable service bundle crate-private.
- Added a source guard that blocks L8F from calling `CommitReplayRuntime`,
  normal commit execution, visible publication, allocator catch-up, or product
  reconstruction hooks.
- Staged checkpoint branch-state replacement until WAL, table validation,
  quarantine inventory, and health aggregation succeed.
- Validate recovered table-object references before WAL tail repair so a missing
  table cannot leave a durable WAL repair side effect.
- Retain validated table identity and table-object facts in the L8F recovery
  package for the L8G/L8J handoff.
- Added a feature-gated lifecycle recovery testkit contract and integration
  test so `tests/lifecycle_recovery.rs` exercises storage behavior, with
  separate counters for canonical smoke paths and script-derived recovery
  coverage.

### Retired From V1 L8F

- Product primitive reconstruction during recovery.
- Direct branch internals mutation for checkpoint rows.
- Treating manifest active WAL segment id as a commit-version watermark.
- Trusting manifest snapshot watermark without loading the snapshot.
- Trusting a manifest flush watermark without recovering the flushed table
  state that proves it.
- Reporting healthy recovery after explicit lossy fallback.

### Deferred By Owner Slice

- L7 WAL replay, allocator/timestamp catch-up, visible-version publication,
  timeline validation, and unresolved durable gate reconciliation: L8G.
- Full generated recovery script counters and fuzz targets: L8O/L8P closeout
  unless pulled forward during L8G.
- Table-backed checkpoint metadata production: L8J. L8F can validate table
  object facts supplied in the recovery request, but current checkpoint tests
  exercise row-native sections.
- Flushed table-state recovery for manifest flush watermarks: L8I/L8J.
- Multi-branch checkpoint installation into runtime branch maps: L8G/L9. L8F
  currently fails closed on checkpoint rows for unopened branches.
- Quarantine mutation, repair, purge, and inventory rewrite: L8M.
- Public open outcome publication: L8G/L9.

### Tests Added

- `recovery_empty_database_returns_healthy_package_without_replay`
- `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail`
- `recovery_does_not_install_checkpoint_when_later_wal_read_fails`
- `recovery_repairs_latest_partial_log_tail_only_when_explicitly_lossy`
- `recovery_rejects_latest_partial_log_tail_in_strict_mode`
- `lossy_missing_snapshot_allows_uncertain_flush_watermark_as_degraded_data_loss`
- `recovery_rejects_checkpoint_row_newer_than_snapshot_watermark`
- `recovery_rejects_checkpoint_rows_for_unopened_branch`
- `recovery_rejects_flush_watermark_without_recovered_table_state`
- `recovery_rejects_missing_referenced_table_object`
- `recovery_records_validated_table_identity_and_facts`
- `recovery_validates_tables_before_wal_tail_repair`
- `recovery_validates_quarantine_before_wal_tail_repair`
- `recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy`
- `recovery_rejects_missing_snapshot_in_strict_mode`
- `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark`
- `recovery_request_rejects_lossy_when_open_plan_is_strict`
- `recovery_request_validates_limits_and_checkpoint_identity`
- `database_manifest_rejects_zero_snapshot_id_before_recovery`
- `recovery_rejects_snapshot_section_count_above_request_limit`
- `checkpoint_row_section_round_trips_and_rejects_trailing_bytes`
- `checkpoint_row_section_rejects_declared_rows_without_length_prefixes`
- `lifecycle_recovery_contract_exercises_storage_recovery_paths`
- `lifecycle_property_harness_runs_recovery_contract`
- `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks`

### Sensitivity Probes Recorded

| Probe | Mutation | Expected failing test |
|---|---|---|
| Trust missing snapshot watermark | Use manifest snapshot watermark after missing snapshot in lossy mode | `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark` |
| Read WAL from zero | Ignore checkpoint watermark when selecting replay start | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Include watermark-equal records | Use `>= replay_start` instead of `> replay_start` | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Skip L6 snapshot install | Decode checkpoint rows but do not call snapshot install | `recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail` |
| Partially mutate shell | Install checkpoint rows before a later WAL failure | `recovery_does_not_install_checkpoint_when_later_wal_read_fails` |
| Skip latest-tail repair | Return truncation without calling repair | `recovery_repairs_latest_partial_log_tail_only_when_explicitly_lossy` |
| Repair latest-tail in strict mode | Run WAL tail repair despite strict recovery | `recovery_rejects_latest_partial_log_tail_in_strict_mode` |
| Repair before table validation | Repair a partial WAL tail before validating a referenced table object | `recovery_validates_tables_before_wal_tail_repair` |
| Repair before quarantine validation | Repair a partial WAL tail before quarantine recovery succeeds | `recovery_validates_quarantine_before_wal_tail_repair` |
| Trust uncovered flush watermark | Use manifest flush watermark without recovered table state | `recovery_rejects_flush_watermark_without_recovered_table_state` |
| Accept too many snapshot sections | Ignore the recovery request section-count cap | `recovery_rejects_snapshot_section_count_above_request_limit` |
| Accept too-new checkpoint row | Install checkpoint row with commit version above snapshot watermark | `recovery_rejects_checkpoint_row_newer_than_snapshot_watermark` |
| Accept unopened branch row | Install checkpoint row for a branch not owned by the shell | `recovery_rejects_checkpoint_rows_for_unopened_branch` |
| Allocate from bogus row count | Reserve row-count capacity before checking payload length | `checkpoint_row_section_rejects_declared_rows_without_length_prefixes` |
| Ignore missing referenced table | Treat missing table object validation as healthy | `recovery_rejects_missing_referenced_table_object` |
| Treat quarantine mismatch as healthy | Ignore corrupt quarantine inventory under lossy policy | `recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy` |
| Healthy lossy fallback | Return `Healthy` after missing snapshot downgrade | `recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark` |
| Collapse source chain | Drop lower snapshot decode/source error | `recovery_rejects_missing_snapshot_in_strict_mode` |
| Accept malformed row section | Ignore trailing checkpoint row bytes | `checkpoint_row_section_round_trips_and_rejects_trailing_bytes` |
| Call L7 replay in L8F | Import or invoke `CommitReplayRuntime` | `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks` |
| Advance visible in L8F | Call visible publication from recovery | `lifecycle_recovery_runtime_does_not_call_commit_replay_or_product_hooks` |

### Verification

Commands run for L8F:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8G - Commit Bootstrap And Recovery Health

Status: implemented

### Source Evidence Read

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/commit/replay.rs`
- `crates/storage-next/src/commit/durable.rs`
- `crates/storage-next/src/commit/allocator.rs`
- `crates/storage-next/src/commit/visibility.rs`
- `crates/storage-next/src/commit/durable_gate.rs`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/durable.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/bootstrap.rs`
- `crates/storage-next/src/testkit/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/tests/recovery.rs`
- `crates/storage-next/tests/lifecycle_recovery.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8g-commit-bootstrap-recovery-health-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

### Preserved As Storage Vocabulary

- L8G consumes only the L8F `LifecycleRecoveryOutcome`; it does not read
  manifests, snapshots, WAL segments, table objects, or quarantine inventory.
- WAL replay is delegated to L7 `CommitReplayRuntime`, preserving row-native
  WAL facts, timeline validation, duplicate/idempotent replay behavior,
  allocator catch-up, visible publication, and unresolved durable-gate
  reconciliation.
- Checkpoint-only recovery uses `VisibleVersionTracker` and
  `CommitFactAllocator` version/timestamp catch-up helpers instead of direct
  field mutation.
- Final durable open facts are reported through `StorageOpenOutcome` after the
  lifecycle state machine accepts `RecoveryAccepted`. The outcome now carries
  backend capabilities, database id, codec id, checkpoint/WAL/table/quarantine
  recovery facts, L7 bootstrap report, and raw stats for the L9 envelope.
- The opened durable runtime remains crate-private and composes normal durable
  commits through `CommitDurableRuntime`.

### Intentional Changes

- Added `LifecycleDurableLocalRuntime` in `lifecycle/durable/bootstrap.rs` as
  the opened durable-local runtime wrapper returned after successful recovery
  bootstrap.
- Added `LifecycleRecoveryBootstrapReport` for storage-shaped replay and
  checkpoint catch-up counters.
- Added `LifecycleDurableLocalShell::complete_recovery`, which consumes a
  recovering shell plus L8F package and returns an open durable runtime.
- Added WAL package validation for branch ownership and strict in-package
  ordering while preserving L7's idempotent replay semantics for checkpoint
  boundary records.
- Added typed `TimelineRecoveryMismatch` mapping for L7 timeline replay errors
  so L8 health/telemetry can distinguish timeline recovery failures from
  generic commit-runtime lower-layer failures.
- Updated lifecycle source guards so durable assembly stays in
  `lifecycle/durable.rs`, L8G replay/catch-up stays in
  `lifecycle/durable/bootstrap.rs`, and L8F remains blocked from replay,
  allocator catch-up, visible publication, and product hooks.

### Retired From V1 L8G

- Reimplementing replay/timeline checks in lifecycle code.
- Opening durable runtime before recovered WAL rows are replayed.
- Publishing checkpoint visibility by mutating visible-version fields directly.
- Accepting timeline-only WAL payloads.
- Treating durable recovery as public API; L9 still owns public wrapping.

### Deferred By Owner Slice

- Multi-branch durable runtime maps and mixed-branch WAL replay: L9 or later L8
  extension.
- Flushed table-state recovery beyond row-native checkpoint install: L8I/L8J.
- Maintenance readiness beyond conservative `false`: L8H+.
- Process-kill crash harnesses across every L8G phase: L8O.
- Durable close drain and sync-on-close: L8N.

### Tests Added

- `bootstrap_empty_recovery_opens_durable_runtime_with_zero_visibility`
- `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator`
- `bootstrap_replays_wal_tail_through_commit_runtime`
- `bootstrap_rejects_timeline_only_wal_payload_before_open`
- `bootstrap_rejects_log_record_without_timeline_rows_before_open`
- `bootstrap_rejects_recovered_log_record_for_unopened_branch`
- `bootstrap_rejects_recovered_log_records_not_strictly_ordered`
- `bootstrap_preserves_degraded_recovery_health_while_replaying_tail`
- `bootstrap_replay_is_idempotent_for_exactly_installed_rows`
- `bootstrap_replay_clears_matching_unresolved_durable_gate`
- `bootstrap_replay_uses_always_durability_for_always_mode`
- `bootstrap_replay_rejects_mismatched_unresolved_durable_gate`
- `lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths`
- `lifecycle_property_harness_runs_bootstrap_contract`
- `lifecycle_durable_runtime_stays_bootstrap_only`
- `lifecycle_bootstrap_runtime_does_not_perform_durable_assembly`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Skip L7 replay | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Install recovered WAL rows directly into L6 | `bootstrap_replays_wal_tail_through_commit_runtime` |
| Ignore checkpoint visible catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not call visible catch-up for checkpoint-only package | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Ignore checkpoint allocator catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not catch allocator above checkpoint watermark | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Ignore checkpoint timestamp catch-up | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not catch timestamp guard up to checkpoint row timestamp max | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Drop durable open facts | `crates/storage-next/src/lifecycle/outcome.rs` | Omit checkpoint/WAL/table/quarantine/bootstrap facts from open outcome | `bootstrap_checkpoint_only_recovery_publishes_visible_and_catches_allocator` |
| Replay with wrong durability | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Map `DurableLocalAlways` recovery to `CommitDurabilityClass::Standard` | `bootstrap_replay_uses_always_durability_for_always_mode` |
| Accept timeline-only WAL | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Remove L7 replay validation or bypass replay request validation | `bootstrap_rejects_timeline_only_wal_payload_before_open` |
| Accept missing timeline rows | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Bypass L7 replay request validation for user rows without timeline facts | `bootstrap_rejects_log_record_without_timeline_rows_before_open` |
| Replay foreign branch | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip recovered WAL branch-ownership validation | `bootstrap_rejects_recovered_log_record_for_unopened_branch` |
| Replay non-increasing package | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip strict recovered WAL in-package order validation | `bootstrap_rejects_recovered_log_records_not_strictly_ordered` |
| Drop degraded health | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Convert degraded L8F health to healthy during open outcome construction | `bootstrap_preserves_degraded_recovery_health_while_replaying_tail` |
| Reapply exact replay | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Treat exact duplicate rows as newly applied during bootstrap replay | `bootstrap_replay_is_idempotent_for_exactly_installed_rows` |
| Ignore matching durable gate | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Do not clear a matching unresolved durable gate after replay | `bootstrap_replay_clears_matching_unresolved_durable_gate` |
| Clear mismatched durable gate | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Clear or ignore an unresolved gate for a different durable fact | `bootstrap_replay_rejects_mismatched_unresolved_durable_gate` |
| Open before recovery accepted | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Skip `RecoveryAccepted` transition | `bootstrap_empty_recovery_opens_durable_runtime_with_zero_visibility` |
| Durable/bootstrap boundary drift | `crates/storage-next/src/lifecycle/durable.rs`, `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Move replay/catch-up into assembly or durable assembly into bootstrap | `lifecycle_durable_runtime_stays_bootstrap_only`, `lifecycle_bootstrap_runtime_does_not_perform_durable_assembly` |

### Verification

Commands run for L8G:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery -- --nocapture
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --test lifecycle_recovery
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8H - Maintenance Task Executor

Status: implemented

### Source Evidence Read

- `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l8-lifecycle-recovery-maintenance-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`
- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/state.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/health.rs`
- `crates/storage-next/src/testkit/lifecycle/`

### Shipped Files

- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/outcome.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance.rs`
- `crates/storage-next/src/lifecycle/tests/maintenance/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/maintenance.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8h-maintenance-task-executor-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md`

### Preserved As Storage Vocabulary

- Maintenance task kinds remain the storage-owned vocabulary introduced by the
  lifecycle scaffold: flush, checkpoint, WAL truncation, compaction,
  materialization, snapshot pruning, retention, quarantine, purge, repair, and
  health collection.
- The executor is deterministic and single-threaded. Ordering is explicit:
  priority first, then enqueue sequence for equal priority.
- Queue capacity is driven by `LifecycleConfig::max_maintenance_queue_depth`.
- Coalescing is explicit through task kind plus storage scope. Duplicate
  requests return a coalesced enqueue outcome instead of pretending a second
  task was queued.
- Close integration is prepared through drain-required and cancel-before-close
  policies, but full durable close sequencing remains owned by L8N.
- Maintenance outcome facts carry task id, status, health debt, affected object
  count, reclaimed bytes, retryability, and stats.
- Runtime maintenance hooks remain crate-private. There is still no public user
  maintenance command surface.

### Raw Health And Fact Vocabulary

- `MaintenanceTaskId` is a deterministic counter; task sequence ordering uses
  the executor's monotonic raw sequence value.
- `MaintenanceTaskPriority` records critical/high/normal/low ordering.
- `MaintenanceTaskScope` records global, branch, WAL, checkpoint, quarantine,
  retention, table-level, and inherited-layer scopes without product DTOs.
- `LifecycleMaintenanceStats` records enqueued, coalesced, started, completed,
  deferred, failed, canceled, drained, and queue-full counters.
- `MaintenanceFaultPoint` records before-enqueue, after-enqueue, at-task-start,
  after-task-run, and during-drain boundaries for deterministic fault tests.
- Maintenance readiness now means an executor is attached and recovery health is
  safe enough for ordinary maintenance. Healthy and telemetry-degraded recovery
  can be ready; data-loss, policy-downgrade, and failed recovery are not ready.

### Intentional Changes

- Cache-mode open now reports maintenance readiness once the executor is
  attached. Durable-only task handlers still defer until later slices; cache
  mode does not import durable services.
- Durable-local open reports maintenance readiness after successful bootstrap
  only when recovery health allows ordinary maintenance.
- Cache close cancels pending cancel-before-close maintenance tasks and reports
  the count through close stats. Durable close drain/sync remains deferred.
- A source guard now rejects architecture-layer labels in lifecycle
  implementation, lifecycle testkit, lifecycle unit tests, and lifecycle
  integration tests, keeping milestone vocabulary in plans instead of code.

### Retired From V1 L8H

- Engine background scheduler imports.
- Product or public manual maintenance command wording.
- Wall-clock sleeps or thread races in executor tests.
- Concrete flush, checkpoint, compaction, retention, quarantine, purge, repair,
  or durable-close implementations inside the executor slice.

### Deferred By Owner Slice

- Flush frozen state and table publication: L8I.
- Checkpoint, flush watermark, and WAL truncation: L8J.
- Compaction and materialization scheduling hooks: L8K.
- Retention proof and snapshot pruning: L8L.
- Quarantine, reclaim, purge, and repair facts: L8M.
- Durable close drain/sync/guard release: L8N.
- Crash/fuzz/fault closeout expansion: L8O-L8P.

### Tests Added

- `maintenance_task_request_validates_kind_scope_pairs`
- `maintenance_task_requests_accept_every_supported_kind_and_scope`
- `maintenance_task_ids_and_sequences_are_monotonic`
- `maintenance_policy_and_coalesce_key_preserve_storage_scope`
- `maintenance_debug_output_uses_storage_vocabulary`
- `maintenance_enqueue_requires_open_and_enforces_capacity`
- `maintenance_admission_rejects_ordinary_work_outside_open`
- `maintenance_close_drain_requires_closing_and_ordinary_run_requires_open`
- `lifecycle_health_query_is_admitted_in_every_state`
- `maintenance_queue_depth_allows_exact_capacity`
- `maintenance_executor_orders_by_priority_then_fifo`
- `maintenance_executor_preserves_fifo_for_equal_priority`
- `maintenance_executor_order_survives_coalescing_and_canceling`
- `maintenance_executor_coalesces_pending_tasks_by_key`
- `maintenance_executor_coalesces_each_coalescing_scope_independently`
- `maintenance_executor_does_not_coalesce_non_coalescing_requests`
- `maintenance_executor_does_not_coalesce_active_task`
- `maintenance_executor_clears_active_after_runner_error`
- `maintenance_executor_run_empty_queue_returns_no_work_without_stats`
- `maintenance_executor_records_deferred_and_preserves_effects`
- `maintenance_executor_converts_after_run_fault_to_failed_outcome`
- `maintenance_executor_attaches_health_debt_to_failed_outcome`
- `maintenance_executor_counts_canceled_outcomes_as_canceled`
- `maintenance_executor_cancel_and_drain_respect_close_policy`
- `maintenance_executor_empty_drain_and_cancel_are_idempotent`
- `maintenance_executor_cancel_removes_only_pending_cancelable_tasks`
- `maintenance_executor_records_drain_fault_without_removing_pending_task`
- `maintenance_fault_before_enqueue_leaves_queue_unchanged`
- `maintenance_fault_after_enqueue_keeps_pending_task_observable`
- `maintenance_fault_hooks_fire_in_deterministic_order`
- `maintenance_ready_policy_tracks_recovery_health_class`
- `cache_runtime_can_enqueue_and_run_health_collection_maintenance`
- `cache_close_rejects_pending_drain_required_maintenance_before_transitioning`
- `bootstrap_runtime_can_enqueue_and_run_health_collection_maintenance`
- `lifecycle_maintenance_contract_covers_executor_categories`
- `lifecycle_property_harness_runs_maintenance_contract`
- `lifecycle_maintenance_tests_avoid_sleeps_and_thread_spawns`
- `lifecycle_implementation_avoids_architecture_labels`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Allow ordinary maintenance outside open | `crates/storage-next/src/lifecycle/maintenance.rs` | Skip lifecycle admission in enqueue/run | `maintenance_enqueue_requires_open_and_enforces_capacity` |
| Ignore queue capacity | `crates/storage-next/src/lifecycle/maintenance.rs` | Remove queue-depth check | `maintenance_enqueue_requires_open_and_enforces_capacity` |
| Reverse priority ordering | `crates/storage-next/src/lifecycle/maintenance.rs` | Select lowest priority first | `maintenance_executor_orders_by_priority_then_fifo` |
| Break FIFO tiebreak | `crates/storage-next/src/lifecycle/maintenance.rs` | Sort equal-priority tasks by newest sequence | `maintenance_executor_orders_by_priority_then_fifo` |
| Drop coalescing fact | `crates/storage-next/src/lifecycle/maintenance.rs` | Return enqueued for duplicate pending task | `maintenance_executor_coalesces_pending_tasks_by_key` |
| Coalesce active task away | `crates/storage-next/src/lifecycle/maintenance.rs` | Match active task in duplicate lookup | `maintenance_executor_does_not_coalesce_active_task` |
| Leave active after error | `crates/storage-next/src/lifecycle/maintenance.rs` | Do not clear active on runner error | `maintenance_executor_clears_active_after_runner_error` |
| Run cancelable task during close drain | `crates/storage-next/src/lifecycle/maintenance.rs` | Drain every pending task regardless of close policy | `maintenance_executor_cancel_and_drain_respect_close_policy` |
| Report ready after data loss | `crates/storage-next/src/lifecycle/maintenance.rs` | Treat every degraded health as ready | `maintenance_ready_policy_tracks_recovery_health_class` |
| Add architecture labels to code | `crates/storage-next/src/lifecycle/*.rs`, `crates/storage-next/src/testkit/lifecycle/*.rs`, `crates/storage-next/tests/lifecycle_*.rs` | Put milestone labels in implementation comments, strings, or test names | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8H:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle
cargo test -p strata-storage-next --all-features --locked --lib lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8J - Checkpoint, Flush Watermark, And WAL Truncation

### Size Note

- This slice exceeded the preferred review-size budget because checkpoint
  publication, flush-watermark persistence, WAL truncation, recovery
  round-trips, and service fault windows landed together. The implementation is
  isolated in `checkpoint.rs` plus checkpoint-specific test modules; future
  checkpoint retention/pruning work should split into smaller owner slices.

### Shipped Files

- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs`
- `crates/storage-next/src/testkit/lifecycle/checkpoint.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8j-checkpoint-watermark-wal-truncation-test-plan.md`

### Preserved As Storage Vocabulary

- Checkpoint requests carry branch id, snapshot id, creation timestamp, optional
  snapshot sections, and explicit follow-up toggles for flush watermark and WAL
  truncation.
- Checkpoint outcomes report checkpoint status, row count, section count,
  snapshot object, active WAL segment, optional flush-watermark facts, optional
  WAL-truncation facts, and recovery health debt.
- Flush-watermark requests use explicit proof vocabulary:
  checkpoint-covered, already-persisted, and table-objects-only.
- WAL truncation accepts only `WalRetentionProof`, preserving source
  vocabulary from snapshot watermark or flush watermark.
- Maintenance tasks enqueue and run checkpoint and WAL-truncation work through
  the common executor and retain task id, task kind, status, retryability,
  effects, stats, and health debt.

### Raw Health And Fact Vocabulary

- `LifecycleCheckpointStatus` records completed, deferred, partial snapshot
  publication, uncertain snapshot visibility, flush-watermark failure, and
  WAL-truncation failure.
- `LifecycleFlushWatermarkStatus` records persisted and already-persisted
  outcomes.
- `LifecycleWalTruncationStatus` records completed and
  completed-with-health-debt outcomes.
- `WalRetentionProofSource` remains the only truncation proof source vocabulary
  that lifecycle can pass downward.
- Checkpoint follow-up failures are represented as maintenance health debt,
  not clean checkpoint success.

### Intentional Changes

- Lifecycle does not scan WAL records or parse segment object names. Coverage
  and active-segment protection remain owned by L4 WAL service logic.
- Checkpoint row capture uses L6 branch row ordering and L7 commit quiesce. The
  checkpoint watermark is the visible version, not the allocator frontier.
- Snapshot publication is delegated to the checkpoint service. Tests pin the
  service order: active-WAL facts, snapshot create, then live snapshot facts.
- Cache mode exposes no checkpoint/flush-watermark/WAL-truncation durable
  claim surface; source guards keep cache lifecycle code away from durable
  services.
- Generated checkpoint coverage tracks input-derived counters separately from
  direct unit tests.

### Retired From V1 L8J

- Old primitive checkpoint section DTOs.
- Product command naming or public maintenance command behavior.
- Direct filesystem/path/object-name parsing for WAL retention.
- Table-object-only flush facts as a replay-shortening proof.
- Logs-only fault handling for partial checkpoint or WAL delete failures.

### Deferred By Owner Slice

- Snapshot pruning after successful checkpoint: L8L.
- Local filesystem checkpoint/recovery integration harness: L8O/L8P.
- Multi-branch public lifecycle wrapper behavior: L9.

### Tests Added

- `checkpoint_task_rejects_wrong_maintenance_scope`
- `checkpoint_rows_include_tombstones_and_timeline_rows`
- `checkpoint_watermark_uses_visible_version_not_allocated_version`
- `checkpoint_snapshot_publish_failure_releases_quiesce_and_keeps_recovery_facts`
- `checkpoint_publishes_snapshot_between_database_record_updates`
- `checkpoint_manifest_publish_failure_reports_partial_snapshot`
- `checkpoint_manifest_uncertainty_reports_uncertain_status`
- `checkpoint_existing_snapshot_id_collision_fails_closed`
- `checkpoint_with_truncation_skips_delete_when_deferred`
- `checkpoint_recovery_restores_rows_without_covered_log_records`
- `checkpoint_recovery_restores_tombstone_and_timeline_rows`
- `flush_watermark_rejects_bounds_and_preserves_branch_state`
- `flush_watermark_persist_failure_preserves_source_chain`
- `wal_truncation_from_checkpoint_and_flush_proofs_are_typed`
- `duplicate_checkpoint_tasks_coalesce_by_checkpoint_scope`
- `queued_checkpoint_task_failure_adds_health_debt`
- `duplicate_wal_truncation_tasks_coalesce_by_retention_scope`
- `lifecycle_property_harness_runs_checkpoint_contract`
- `lifecycle_checkpoint_runtime_avoids_segment_parsing_and_direct_delete`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Quiesce omitted | `crates/storage-next/src/lifecycle/checkpoint.rs` | Remove commit quiesce before checkpoint row capture | `checkpoint_reads_visible_version_after_commit_quiesce` |
| Allocator watermark used | `crates/storage-next/src/lifecycle/checkpoint.rs` | Use allocator frontier instead of visible version | `checkpoint_watermark_uses_visible_version_not_allocated_version` |
| Hidden rows captured | `crates/storage-next/src/branch/state.rs` | Include rows above checkpoint watermark | `checkpoint_rows_include_owned_frozen_active_and_exclude_newer_rows` |
| Tombstones dropped | `crates/storage-next/src/branch/state.rs` | Filter tombstone rows from checkpoint rows | `checkpoint_rows_include_tombstones_and_timeline_rows` |
| Timeline rows dropped | `crates/storage-next/src/branch/state.rs` | Filter timeline rows from checkpoint rows | `checkpoint_rows_include_tombstones_and_timeline_rows` |
| Snapshot/manifest order inverted | `crates/storage-next/src/service/checkpoint.rs` | Persist snapshot facts before snapshot create | `checkpoint_publishes_snapshot_between_database_record_updates` |
| Partial snapshot marked success | `crates/storage-next/src/lifecycle/checkpoint.rs` | Collapse orphan snapshot status to completed | `checkpoint_manifest_publish_failure_reports_partial_snapshot` |
| Table-only flush proof accepted | `crates/storage-next/src/lifecycle/checkpoint.rs` | Allow table-only proof in flush watermark persistence | `flush_watermark_proofs_are_conservative_and_monotonic` |
| Branch absence advances watermark | `crates/storage-next/src/lifecycle/checkpoint.rs` | Treat no rows as flush proof | `checkpoint_defers_when_branch_has_no_rows_under_visible_watermark` |
| Primitive truncation watermark | `crates/storage-next/src/lifecycle/checkpoint.rs` | Replace typed retention proof with raw commit version | `wal_truncation_from_checkpoint_and_flush_proofs_are_typed` |
| Active segment deleted | `crates/storage-next/src/service/wal.rs` | Remove active-segment protection in covered delete | `checkpoint_recovery_restores_rows_without_covered_log_records` |
| Delete failure ignored | `crates/storage-next/src/lifecycle/checkpoint.rs` | Return clean success after WAL delete error | `checkpoint_reports_wal_truncation_failure_without_losing_snapshot_facts` |
| Cache mode creates durable facts | `crates/storage-next/src/lifecycle/cache.rs` | Import durable services into cache lifecycle | `lifecycle_cache_runtime_stays_cache_only` |
| Old primitive DTO imported | `crates/storage-next/src/lifecycle/*.rs` | Reintroduce old checkpoint DTO vocabulary | `lifecycle_source_does_not_import_engine_product_or_raw_io` |
| Architecture label added to code | `crates/storage-next/src/lifecycle/*.rs`, `crates/storage-next/src/testkit/lifecycle/*.rs`, `crates/storage-next/tests/lifecycle_*.rs` | Put milestone labels in implementation comments, strings, or test names | `lifecycle_implementation_avoids_architecture_labels` |

### Verification

Commands run for L8J:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::checkpoint
cargo test -p strata-storage-next --locked --lib lifecycle::tests::maintenance
cargo test -p strata-storage-next --locked --lib lifecycle::tests::recovery
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --all-features --locked --test object_layout_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8L - Retention Proof And Snapshot Pruning

### Shipped Files

- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/tests/retention.rs`
- `crates/storage-next/src/testkit/lifecycle/retention.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8l-retention-proof-snapshot-pruning-test-plan.md`

### Preserved As Storage Vocabulary

- Retention proof distinguishes complete, incomplete, and
  recovery-health-blocked states.
- Snapshot pruning keeps the manifest-live snapshot, keeps the newest retained
  snapshot window, and clamps zero retain count to one.
- Snapshot pruning requires manifest live-snapshot facts even when the current
  snapshot listing is empty; an empty listing is not treated as a durable safety
  proof.
- Snapshot delete failures become health debt while preserving successfully
  deleted and protected snapshot facts.
- Table objects are classified as retained or quarantine candidates. Lifecycle
  retention does not delete table objects directly. Automatic table-reachability
  proof assembly remains deferred until durable table-manifest/quarantine work.
- WAL and quarantine object families are delegated with explicit skipped
  decisions rather than partially implemented in retention. These intentional
  delegations are not health debt.

### Raw Health And Fact Vocabulary

- `LifecycleRetentionProofStatus` records complete, incomplete, and
  blocked-by-recovery-health proof states.
- `LifecycleRetentionDecisionRecord` records object family, decision, optional
  object name, and storage-shaped reason.
- `LifecycleSnapshotPruningOutcome` records deleted, protected, and failed
  snapshot objects and converts failed deletes into telemetry health debt.
- Maintenance outcomes preserve affected object names, state-change counts,
  source chains for service errors, and retention-block stats.

### Intentional Changes

- Snapshot deletion is delegated exclusively to `SnapshotService::prune_snapshots`.
- Retention code never parses WAL segments, truncates WAL objects, mutates
  quarantine inventory, or deletes table objects.
- Retention task coalescing includes the snapshot-retain policy so explicit
  pruning windows are not lost.
- Global retention maintenance runs snapshot pruning and still reports delegated
  WAL/quarantine families rather than returning only delegated decisions.
- Successful delegated WAL/quarantine decisions no longer inflate recovery
  health; only incomplete proofs or real lower-layer failures add health debt.
- Cache mode rejects durable retention and snapshot-pruning tasks before any
  durable-object access.

### Retired From V1 L8L

- Product retention reports and branch-attribution DTOs.
- Direct filesystem/path deletion.
- Logs-only snapshot pruning diagnostics.
- Table-object purge and quarantine mutation.
- Row-version pruning policy.
- WAL segment parsing or deletion from retention code.

### Deferred By Owner Slice

- Quarantine inventory publication, movement, purge, and repair: L8M.
- Close-time retention drain: L8N.
- Crash/fuzz assurance expansion: L8O/L8P.
- Public retention commands and product reports: L9.
- Automatic table-reachability proof assembly and table-manifest-backed direct
  table-object deletion, if ever allowed: later durable table-manifest work.

### Tests Added

- `retention_request_accepts_zero_snapshot_retain_as_clamped_policy`
- `retention_proof_incomplete_without_manifest_snapshot_when_snapshots_exist`
- `retention_proof_incomplete_without_manifest_snapshot_even_when_listing_empty`
- `retention_proof_incomplete_without_branch_reachability_for_tables`
- `incomplete_snapshot_pruning_proof_defers_before_backend_access`
- `retention_proof_blocks_data_loss_before_backend_access`
- `retention_scope_snapshot_decisions_respect_live_and_newest_windows`
- `global_retention_scope_includes_snapshot_and_delegated_decisions`
- `snapshot_pruning_retains_live_snapshot_outside_newest_window`
- `snapshot_pruning_clamps_zero_retain_count_to_one`
- `snapshot_pruning_delete_failure_records_health_debt_and_continues`
- `snapshot_pruning_list_failure_preserves_service_source_chain`
- `table_object_retention_classifies_quarantine_candidate_without_backend_delete`
- `retention_delegates_wal_and_quarantine_families`
- `snapshot_pruning_tasks_coalesce_by_retain_policy`
- `global_retention_task_prunes_snapshots_through_durable_maintenance`
- `prove_retention_respects_snapshot_scope_without_deleting`
- `cache_runtime_rejects_durable_retention_tasks_before_backend_access`
- `lifecycle_retention_proof_integration`
- `lifecycle_snapshot_pruning_integration`
- `lifecycle_table_retention_delegation_integration`
- `lifecycle_retention_source_delegates_durable_mutation`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Retain count zero deletes all | `crates/storage-next/src/lifecycle/retention.rs` | Pass zero directly as "retain none" | `snapshot_pruning_clamps_zero_retain_count_to_one` |
| Live snapshot not protected | `crates/storage-next/src/lifecycle/retention.rs` | Drop live snapshot id before pruning | `snapshot_pruning_retains_live_snapshot_outside_newest_window` |
| Empty listing trusted without manifest | `crates/storage-next/src/lifecycle/retention.rs` | Treat empty snapshot listing as complete proof without manifest facts | `retention_proof_incomplete_without_manifest_snapshot_even_when_listing_empty` |
| Global retention skips snapshots | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Route global retention only to delegated WAL/quarantine decisions | `global_retention_task_prunes_snapshots_through_durable_maintenance` |
| Scope ignored by proof hook | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Return delegated WAL/quarantine decisions for snapshot-only proof requests | `prove_retention_respects_snapshot_scope_without_deleting` |
| Incomplete proof deletes | `crates/storage-next/src/lifecycle/retention.rs` | Call snapshot service when proof is incomplete | `incomplete_snapshot_pruning_proof_defers_before_backend_access` |
| Data-loss recovery prunes | `crates/storage-next/src/lifecycle/retention.rs` | Treat data-loss health as safe | `retention_proof_blocks_data_loss_before_backend_access` |
| Delete failure hidden | `crates/storage-next/src/lifecycle/retention.rs` | Collapse failed deletes into completed outcome | `snapshot_pruning_delete_failure_records_health_debt_and_continues` |
| Service source chain dropped | `crates/storage-next/src/lifecycle/retention.rs` | Convert list failure into string-only error | `snapshot_pruning_list_failure_preserves_service_source_chain` |
| Table object deleted directly | `crates/storage-next/src/lifecycle/retention.rs` | Call backend delete for table candidates | `lifecycle_retention_source_delegates_durable_mutation` |
| WAL truncation in retention | `crates/storage-next/src/lifecycle/retention.rs` | Call WAL segment deletion from retention | `lifecycle_retention_source_delegates_durable_mutation` |
| Quarantine mutation in retention | `crates/storage-next/src/lifecycle/retention.rs` | Call quarantine mutation/purge APIs | `lifecycle_retention_source_delegates_durable_mutation` |
| Retain policy coalesced away | `crates/storage-next/src/lifecycle/maintenance.rs` | Ignore retain options in coalesce key | `snapshot_pruning_tasks_coalesce_by_retain_policy` |

### Verification

Commands run for L8L:

```bash
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --all-features --locked --test lifecycle_source_guard
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

## L8S Table-Object Reachability And Retention

### Scope

- Added a lifecycle table-object reachability proof surface that consumes
  trusted table manifests, table-object inventory, quarantine inventory, and
  recovery health.
- The slice only classifies storage objects. It does not delete table objects,
  move them to quarantine, rewrite quarantine inventory, update the database
  manifest, checkpoint, or truncate WAL.

### Shipped Files

- `crates/storage-next/src/lifecycle/table_reachability.rs`
- `crates/storage-next/src/lifecycle/tests/table_object_retention.rs`
- `crates/storage-next/src/lifecycle/tests/table_object_retention/plan.rs`
- `crates/storage-next/src/lifecycle/retention.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`

### Preserved As Storage Vocabulary

- Manifest-referenced table objects are retained.
- Inherited-layer table objects are retained even when they are not owned by
  the child branch.
- Shared table objects remain retained while any trusted manifest still
  references them.
- Prefix-listed table objects are not live unless a trusted manifest references
  them.
- Orphan table objects become quarantine candidates only when manifest,
  table-inventory, quarantine-inventory, and recovery-health facts are complete
  and safe.
- Already-quarantined table objects are delegated to quarantine/purge handling
  instead of being re-queued.

### Raw Health And Fact Vocabulary

- `LifecycleTableObjectProofEpochs` records freshness for manifest,
  table-inventory, quarantine-inventory, and recovery-health facts.
- `LifecycleTableObjectProofToken` binds a quarantine candidate to the current
  branch, proof epochs, and object-fact fingerprint.
- `LifecycleTableObjectRetentionOutcome` wraps retention decisions and exposes
  candidate tokens for the quarantine boundary.
- `LifecycleRetentionDecisionReason` now distinguishes inherited,
  materialized, shared, already-quarantined, and malformed table-object cases.

### Intentional Changes

- The old `LifecycleRetentionScope::TableObjects` generic retention path
  remains an explicit unsupported/deferred scope; the new table-object path
  requires the dedicated reachability request so callers cannot accidentally
  infer reachability from an incomplete generic proof.
- Table-manifest objects under the table namespace and non-table object families
  are ignored by table-object retention.
- Policy-downgrade, data-loss, and failed recovery health block table-object
  quarantine candidates; unrelated telemetry degradation is allowed unless the
  caller disables it.

### Retired From V1 L8S

- Product retention reports and branch-name attribution.
- Runtime-only reference registries as durable truth.
- Direct backend delete/quarantine/purge calls from reachability code.
- Row-version pruning.

### Tests Added

- `manifest_referenced_table_object_is_retained`
- `shared_table_object_is_retained_until_all_manifest_refs_drop`
- `inherited_layer_table_object_is_retained`
- `materialization_replacement_table_object_is_retained`
- `orphaned_table_object_becomes_quarantine_candidate_with_fresh_token`
- `stale_quarantine_token_rejects_changed_inventory_epoch`
- `proof_token_rejects_manifest_epoch_change`
- `proof_token_rejects_quarantine_inventory_epoch_change`
- `proof_token_rejects_recovery_health_epoch_change`
- `proof_token_rejects_object_fingerprint_change`
- `already_quarantined_table_object_is_not_requeued`
- `incomplete_manifest_proof_retains_all_inventory_until_retry`
- `missing_manifest_referenced_inventory_keeps_proof_incomplete`
- `no_table_objects_returns_completed_empty_graph`
- `policy_downgrade_blocks_table_object_candidate`
- `data_loss_recovery_health_blocks_table_object_retention`
- `failed_health_blocks_table_object_candidate`
- `telemetry_degraded_recovery_health_can_still_classify_candidates`
- `telemetry_degraded_recovery_health_blocks_when_policy_disallows_it`
- `malformed_table_prefix_object_is_quarantine_candidate`
- `table_manifest_and_non_table_inventory_objects_are_ignored`
- `shuffled_inventory_produces_deterministic_decision_order`
- `duplicate_inventory_entry_is_rejected`
- `zero_epoch_is_rejected`
- `quarantine_candidate_can_build_quarantine_proof`
- `runtime_only_ref_does_not_make_object_live`
- `table_object_health_debt_is_empty_for_complete_healthy_outcome`
- `table_object_retention/plan.rs` closeout matrix:
  inventory ordering and malformed-object classification; manifest-owned,
  inherited, materialized, and shared reachability; unsupported table-object
  scope behavior; cache-mode rejection; unsafe-health barriers; proof-token
  epoch/fingerprint binding; no-delete/no-quarantine/no-purge/no-checkpoint/no
  WAL side-effect guarantees; and old-regression shapes for runtime-only refs,
  corrupt/missing manifest health, and shared-object manifest drops.
- `check_lifecycle_retention_contract` table-reachability counters:
  live-owned, live-inherited, live-shared, orphan-candidate, incomplete proof,
  unsafe-health block, already-quarantined, stale-token rejection, and no
  mutation observed.
- `lifecycle_table_reachability_source_is_classification_only`
- Dedicated table-reachability source guards for raw IO, backend delete,
  quarantine mutation, purge, product/engine crates, StrataHub vocabulary,
  primitive modules, and product retention reports.
- `generated_table_object_reachability_covers_ordering_and_safety_categories`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Prefix object treated as live | `crates/storage-next/src/lifecycle/table_reachability.rs` | Classify inventory objects as `Retain` without manifest refs | `runtime_only_ref_does_not_make_object_live` |
| Direct delete from reachability | `crates/storage-next/src/lifecycle/table_reachability.rs` | Call backend delete for orphan candidates | `lifecycle_table_reachability_source_is_classification_only` |
| Inherited refs ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Skip inherited-layer manifest tables | `inherited_layer_table_object_is_retained` |
| Shared refs ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Replace duplicate live refs instead of marking shared | `shared_table_object_is_retained_until_all_manifest_refs_drop` |
| Data-loss health allowed | `crates/storage-next/src/lifecycle/table_reachability.rs` | Treat data-loss health as safe | `data_loss_recovery_health_blocks_table_object_retention` |
| Manifest epoch ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Omit manifest epoch from proof token validation | `proof_token_rejects_manifest_epoch_change` |
| Inventory epoch ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Omit table-inventory epoch from proof token validation | `stale_quarantine_token_rejects_changed_inventory_epoch` |
| Health epoch ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Omit recovery-health epoch from proof token validation | `proof_token_rejects_recovery_health_epoch_change` |
| Object facts ignored | `crates/storage-next/src/lifecycle/table_reachability.rs` | Omit inventory byte count from proof fingerprint | `proof_token_rejects_object_fingerprint_change` |
| Decision order unstable | `crates/storage-next/src/lifecycle/table_reachability.rs` | Preserve caller inventory order | `shuffled_inventory_produces_deterministic_decision_order` |

### Verification

Commands run for L8S (all passed):

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::retention
cargo test -p strata-storage-next --locked --lib lifecycle::tests::table_object_retention
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8T - Table-Manifest-Backed Flush Watermarks

### Shipped Files

- `crates/storage-next/src/lifecycle/checkpoint.rs`
- `crates/storage-next/src/lifecycle/facts.rs`
- `crates/storage-next/src/lifecycle/maintenance.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/lifecycle/durable/close.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/tests/flush_watermark.rs`
- `crates/storage-next/src/lifecycle/tests/flush_watermark/remaining.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/remaining.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Flush watermarks remain database-manifest facts.
- WAL truncation still delegates to the WAL service through
  `WalRetentionProof`; lifecycle does not parse WAL segment objects.
- Table objects alone are not a recovery proof.
- Table-manifest publication uncertainty is not a recovery proof.
- Checkpoint coverage and table-manifest coverage can both contribute to a
  trusted replay boundary, but recovery validates coverage before choosing the
  WAL replay start.

### Raw Health And Fact Vocabulary

- `LifecycleTableManifestFlushCoverageProof` binds a candidate watermark to
  manifest epoch, recovery-health epoch, and branch coverage.
- `LifecycleTableManifestBranchCoverage` records the covered commit range,
  manifest object, table count, and storage-row family coverage for a branch.
- `LifecycleTableManifestCoverageFamilies` records required storage-row
  families with bit flags, so missing tombstone, timeline, inherited-layer, or
  materialized-replacement coverage fails closed.

### Intentional Changes

- Recovery now accepts a database flush watermark above checkpoint coverage only
  when recovered table-manifest facts cover the watermark.
- Recovery starts WAL replay at the validated flush watermark when table
  manifests prove the covered rows are recoverable.
- When checkpoint and table-manifest state are both present and the flush
  watermark depends on table manifests, checkpoint rows must also be present in
  the recovered table-manifest branch state before that state can become the
  recovery base.
- Durable runtime gained an explicit table-manifest-backed flush-watermark hook;
  it loads the already-published branch table manifest and does not publish a
  new table manifest as part of watermark persistence.
- The maintenance executor now has an explicit flush-watermark task kind keyed
  by candidate version. It coalesces by candidate, reports deferred coverage
  gaps as maintenance outcomes, and keeps checkpoint execution separate.

### Retired From V1 L8T

- Persisting flush watermarks from table-object publication alone.
- Treating a missing branch as coverage.
- Inferring timeline/tombstone coverage from a table object's commit max alone.
- Moving table-manifest publication into checkpoint execution.

### Tests Added

- `table_manifest_flush_proof_accepts_exact_coverage`
- `table_manifest_flush_proof_rejects_missing_branch_coverage`
- `table_manifest_flush_proof_rejects_stale_manifest_epoch`
- `table_manifest_flush_proof_rejects_stale_recovery_health_epoch`
- `table_manifest_flush_proof_is_deterministic_for_shuffled_inputs`
- `table_manifest_flush_proof_rejects_active_rows_below_candidate`
- `table_manifest_flush_proof_rejects_frozen_rows_below_candidate`
- `table_manifest_coverage_rejects_timeline_gap`
- `table_manifest_coverage_rejects_tombstone_gap`
- `unsafe_recovery_health_blocks_table_manifest_flush_proof`
- `flush_watermark_persists_from_table_manifest_coverage`
- `flush_watermark_persists_from_combined_checkpoint_and_table_manifest_coverage`
- `flush_watermark_rejects_table_manifest_candidate_above_coverage`
- `flush_watermark_success_does_not_publish_table_manifest`
- `durable_runtime_persists_table_manifest_flush_watermark_after_flush`
- `recovery_accepts_flush_watermark_above_checkpoint_when_table_manifest_covers`
- `recovery_rejects_flush_watermark_above_table_manifest_coverage`
- `recovery_after_truncation_restores_latest_reads`
- `table_manifest_flush_proof_accepts_coverage_above_checkpoint`
- `table_manifest_flush_proof_rejects_table_object_without_manifest`
- `table_manifest_flush_proof_rejects_manifest_publish_uncertain`
- `table_manifest_flush_proof_rejects_candidate_above_visible_version`
- `table_manifest_flush_proof_rejects_zero_candidate`
- `table_manifest_coverage_includes_user_rows`
- `table_manifest_coverage_includes_tombstones`
- `table_manifest_coverage_includes_timeline_rows`
- `table_manifest_coverage_includes_materialized_replacement_rows`
- `table_manifest_coverage_includes_inherited_layer_rows`
- `table_manifest_coverage_rejects_inherited_layer_gap`
- `flush_watermark_rejects_table_manifest_candidate_below_current_as_stale`
- `flush_watermark_equal_to_current_is_noop`
- `flush_watermark_persist_failure_prevents_wal_truncation`
- `flush_watermark_success_records_manifest_fact`
- `flush_watermark_success_does_not_mutate_branch_state`
- `cache_mode_rejects_table_manifest_flush_watermark`
- `recovery_rejects_missing_table_manifest_for_flush_watermark`
- `recovery_rejects_corrupt_table_manifest_for_flush_watermark`
- `recovery_rejects_table_object_mismatch_for_flush_watermark`
- `recovery_uses_table_manifest_flush_watermark_as_replay_start_after_validation`
- `recovery_replays_wal_tail_above_table_manifest_flush_watermark`
- `recovery_ignores_duplicate_record_at_table_manifest_flush_watermark`
- `recovery_after_truncation_restores_history_reads_within_retained_bounds`
- `policy_downgrade_blocks_table_manifest_flush_proof`
- `data_loss_blocks_table_manifest_flush_proof`
- `telemetry_health_allows_unrelated_table_manifest_flush_proof`
- `table_manifest_reachability_debt_blocks_flush_proof`
- `quarantine_inventory_mismatch_blocks_flush_proof_when_relevant`
- `missing_branch_lifecycle_fact_blocks_absence_coverage`
- `branch_absence_does_not_advance_flush_watermark`
- `maintenance_task_can_request_table_manifest_flush_watermark`
- `maintenance_task_coalesces_table_manifest_flush_watermark_by_candidate`
- `maintenance_task_reports_deferred_when_table_coverage_missing`
- `maintenance_task_reports_health_debt_on_wal_truncation_failure`
- `maintenance_task_does_not_run_table_manifest_watermark_after_close_begins`
- `maintenance_task_preserves_stats_for_watermark_and_truncation`
- `maintenance_task_does_not_claim_checkpoint_execution`
- `wal_truncation_from_table_manifest_flush_watermark_uses_typed_proof`
- `wal_truncation_from_table_manifest_flush_watermark_deletes_covered_segments`
- `wal_truncation_keeps_segment_with_record_above_table_manifest_watermark`
- `wal_truncation_keeps_active_segment_under_table_manifest_watermark`
- `wal_truncation_keeps_newer_than_active_segment`
- `wal_truncation_partial_delete_report_preserves_source_chain`
- `table_manifest_watermark_publish_uses_canonical_manifest_bytes`
- Source guards:
  `table_manifest_watermark_does_not_import_raw_io`,
  `table_manifest_watermark_does_not_scan_wal_segments`,
  `wal_truncation_does_not_parse_wal_objects_in_lifecycle`,
  `table_manifest_watermark_does_not_decode_table_bytes_directly`,
  `table_manifest_watermark_does_not_import_backend_delete`,
  `table_manifest_watermark_does_not_import_engine_or_product_crates`,
  `table_manifest_watermark_does_not_import_stratahub`,
  `table_manifest_watermark_does_not_import_primitive_modules`,
  `cache_mode_does_not_import_table_manifest_watermark_runner`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Accept table objects as proof | `crates/storage-next/src/lifecycle/checkpoint.rs` | Treat `TableObjectsOnly` as accepted coverage | `flush_watermark_proofs_are_conservative_and_monotonic` |
| Ignore active rows | `crates/storage-next/src/lifecycle/checkpoint.rs` | Skip active-row check while building table-manifest proof | `table_manifest_flush_proof_rejects_active_rows_below_candidate` |
| Ignore frozen rows | `crates/storage-next/src/lifecycle/checkpoint.rs` | Skip frozen-row check while building table-manifest proof | `table_manifest_flush_proof_rejects_frozen_rows_below_candidate` |
| Ignore proof epoch | `crates/storage-next/src/lifecycle/checkpoint.rs` | Accept stale manifest/recovery-health epochs | `table_manifest_flush_proof_rejects_stale_manifest_epoch`, `table_manifest_flush_proof_rejects_stale_recovery_health_epoch` |
| Ignore timeline coverage | `crates/storage-next/src/lifecycle/checkpoint.rs` | Allow proof with timeline-family bit cleared | `table_manifest_coverage_rejects_timeline_gap` |
| Ignore tombstone coverage | `crates/storage-next/src/lifecycle/checkpoint.rs` | Allow proof with tombstone-family bit cleared | `table_manifest_coverage_rejects_tombstone_gap` |
| Allow unsafe health | `crates/storage-next/src/lifecycle/checkpoint.rs` | Treat data-loss recovery health as safe for table-manifest proof | `unsafe_recovery_health_blocks_table_manifest_flush_proof` |
| Start replay from untrusted flush | `crates/storage-next/src/lifecycle/recovery.rs` | Use manifest flush watermark before table-manifest validation | `recovery_rejects_flush_watermark_above_table_manifest_coverage` |
| Replay from checkpoint only | `crates/storage-next/src/lifecycle/recovery.rs` | Ignore validated table-manifest flush watermark when choosing replay start | `recovery_accepts_flush_watermark_above_checkpoint_when_table_manifest_covers` |
| Publish table manifest during watermark persist | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Call table-manifest publication from the watermark hook | `flush_watermark_success_does_not_publish_table_manifest` |

### Verification

Commands run for L8T (all passed):

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib lifecycle::tests::flush_watermark
cargo test -p strata-storage-next --locked --lib lifecycle::tests
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8V Retention-Aware Row Pruning

Status: branch-runtime implementation, lifecycle durable rewrite coverage,
generated contract coverage, source guards, durable retained-history
manifest extension, cross-branch shared-table proof gate, live
recovery-health attestation, source-identity binding in the fingerprint,
and typed `BranchCompactionInvalidity` error vocabulary all landed.
Typed `getv`/history boundary errors below the retained version floor
remain deferred to the public retained-history read closeout work.

### Shipped Files

- `crates/storage-next/src/branch/pruning.rs`
- `crates/storage-next/src/branch/mod.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/error.rs`
- `crates/storage-next/src/branch/tests.rs`
- `crates/storage-next/src/branch/tests/row_pruning.rs`
- `crates/storage-next/src/branch/tests/row_pruning/required_plan.rs`
- `crates/storage-next/src/branch/tests/row_pruning/tombstone_ttl.rs`
- `crates/storage-next/src/branch/tests/inheritance_materialization/validation_fork.rs`
- `crates/storage-next/src/format/mod.rs`
- `crates/storage-next/src/lifecycle/compaction.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/recovery.rs`
- `crates/storage-next/src/lifecycle/retained_history_extension.rs`
- `crates/storage-next/src/lifecycle/table_manifest.rs`
- `crates/storage-next/src/lifecycle/tests/compaction/mod.rs`
- `crates/storage-next/src/lifecycle/tests/compaction/row_pruning.rs`
- `crates/storage-next/src/testkit/lifecycle/row_pruning.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/lifecycle_maintenance.rs`
- `crates/storage-next/tests/lifecycle_properties.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`

### Preserved As Storage Vocabulary

- Row pruning is proof-gated. Existing keep-all compaction remains the default.
- Branch compaction still rejects pruning policies when no proof is attached.
- The proof is branch-scoped, fingerprint-bound to current branch state, and
  rejected when state changes before output build.
- Retained version floors keep rows at or above the floor and keep a
  below-floor survivor for each key.
- Tombstone elision is conservative: bottommost proof is required, inherited
  layers must be absent/proven safe, and elision rejects when an older value
  in the rewrite input could be resurrected.
- TTL pruning uses an explicit cutoff timestamp. It does not consult wall
  clock time or a global TTL index.

### Raw Health And Fact Vocabulary

- `BranchCompactionPruningProof` records the retained version floor,
  optional timestamp floor, branch state fingerprint, recovery-health epoch,
  table-manifest coverage floor, inherited-layer proof, tombstone proof, TTL
  proof, and optional max versions per key.
- `BranchCompactionPruningPolicy` adapts proof facts into L5
  `TableCompactionPolicy` decisions and preserves the L5 drop-summary
  vocabulary: older-version, tombstone-elided, and expired.

### Intentional Changes

- `BranchCompactionRequest` now accepts an optional pruning proof alongside
  the existing retention-policy enum.
- `LifecycleCompactionRequest` can carry the same proof into cache or durable
  rewrite paths, but durable publication mechanics stay in the existing table
  rewrite module.
- Successful pruning with a retained timestamp floor narrows branch timestamp
  coverage to `CompleteSince(floor)` instead of silently widening history.
- Source guards now cover the row-pruning path and reject raw IO, object
  deletion, WAL truncation, wall-clock TTL policy, product modules, and
  milestone-label leakage.
- Durable rewrite tests cover pruned table-manifest publication, checkpoint
  recovery of retained rows, rejection of pruned user history, materialization
  recovery, and WAL-tail replay after checkpointing pruned state.

### Deferred From This Implementation Pass

- Typed version-history boundary errors for `getv`/history below retained
  version floor (still surface as `InsufficientTimestampHistory` for
  timestamp-bound reads; version-bound reads return `None` and a future
  slice will add the typed boundary).
- A public lifecycle retention-proof builder that constructs the proof
  from a live `RecoveryHealth` reference end-to-end. The branch-layer
  proof carries an explicit `BranchRecoveryHealthAttestation::Healthy`
  attestation and the lifecycle layer is responsible for setting it
  only when actual `RecoveryHealth::Healthy` is observed; sealed-in
  helper is left for the lifecycle slice that adds the public retention
  API.

### Implemented In Follow-Up Fix Pass

- `BranchCompactionInvalidity` typed reason enum with stable
  `failed_precondition.branch.row_pruning_*` codes; `InvalidCompaction`
  no longer carries a free-form string. Existing call sites use
  `BranchCompactionInvalidity::Generic("...")` for non-pruning
  validation failures.
- `BranchSharedTableSafety::{Unknown, NotShared}` proof field with a
  `derive_shared_table_safety` helper that consults
  `SharedTableRegistry`. Pruning rejects without an explicit
  `NotShared` attestation.
- `BranchRecoveryHealthAttestation::{Unknown, Healthy}` proof field;
  pruning rejects without an explicit `Healthy` attestation.
- `branch_pruning_fingerprint` now hashes
  `BranchOwnedTable::materialization_source`, so a proof built before
  materialization fails the post-materialization fingerprint check
  ("source identity, not layer index").
- `from_branch_state` derives `visible_version` from
  `branch.max_commit_version()`, defaulting to the floor when the
  branch is empty. `validate_for_branch` rejects callers that lied
  low about `visible_version`.
- `lifecycle::retained_history_extension` encodes/decodes a
  `storage.retained_history` manifest extension section. The lifecycle
  manifest writer emits the extension whenever the branch's
  `BranchTimestampCoverage` is `CompleteSince`; the recovery path
  decodes it and reapplies the narrowed coverage to the branch state on
  reopen — including the checkpoint-priority path where the manifest
  is not the row source.

### Tests Added

- `row_pruning_request_without_proof_rejects`
- `row_pruning_proof_branch_mismatch_rejects`
- `row_pruning_proof_degraded_recovery_rejects`
- `row_pruning_proof_retained_floor_above_visible_rejects`
- `row_pruning_proof_timestamp_floor_without_coverage_rejects`
- `row_pruning_proof_active_view_below_floor_rejects`
- `row_pruning_proof_pinned_view_below_floor_rejects`
- `row_pruning_proof_inherited_layer_unknown_rejects`
- `row_pruning_proof_zero_floor_keeps_all`
- `row_pruning_proof_is_deterministic_for_shuffled_facts`
- `version_pruning_keeps_retained_rows_and_floor_survivor`
- `version_pruning_preserves_getv_within_floor`
- `version_pruning_as_of_below_floor_returns_insufficient_history`
- `version_pruning_non_monotone_timestamps_respects_timestamp_floor`
- `max_versions_keeps_newest_n_versions`
- `max_versions_zero_means_unbounded_when_floor_keeps_all`
- `max_versions_counts_values_but_not_required_tombstones`
- `row_pruning_proof_stale_epoch_rejects_without_mutation`
- `tombstone_pruning_rejects_resurrection_risk`
- `tombstone_pruning_rejects_without_elision_proof`
- `bottommost_tombstone_below_floor_can_be_elided`
- `non_bottommost_tombstone_below_floor_is_kept`
- `tombstone_above_floor_is_kept`
- `tombstone_needed_to_shadow_inherited_value_is_kept`
- `expired_row_pruning_uses_supplied_cutoff`
- `ttl_pruning_rejects_without_ttl_proof`
- `expired_ttl_above_version_floor_is_kept`
- `expired_ttl_needed_by_as_of_timestamp_rejects_cutoff`
- `non_expired_ttl_row_is_kept`
- `ttl_pruning_across_inherited_parent_child_keeps_required_parent_row`
- Required-plan wrappers for version pruning, max-version pruning,
  tombstone/TTL pruning, inherited-layer safety, materialization precedence,
  and cache-mode volatile coverage.
- Lifecycle durable rewrite tests:
  `durable_pruned_compaction_publishes_pruned_manifest_facts`,
  `manifest_records_retained_version_floor`,
  `manifest_records_retained_timestamp_floor`,
  `durable_pruned_compaction_recovery_restores_retained_reads`,
  `durable_pruned_compaction_recovery_rejects_pruned_history`,
  `durable_pruned_materialization_recovery_preserves_retained_reads`,
  `manifest_missing_pruning_facts_rejects_recovery`,
  `wal_tail_replay_after_pruned_manifest_preserves_newer_rows`,
  `checkpoint_after_pruning_preserves_coverage_boundary`.
- Generated/property coverage:
  `generated_row_pruning_covers_retained_history_boundaries`,
  `lifecycle_property_harness_runs_row_pruning_contract`.
- Source guards:
  `row_pruning_does_not_import_raw_io`,
  `row_pruning_does_not_import_backend_delete`,
  `row_pruning_does_not_import_quarantine_or_purge`,
  `row_pruning_does_not_import_snapshot_pruning`,
  `row_pruning_does_not_import_wal_truncation`,
  `row_pruning_does_not_import_product_policy`,
  `row_pruning_does_not_import_stratahub`,
  `row_pruning_does_not_import_primitive_modules`,
  `row_pruning_does_not_use_wall_clock`,
  `row_pruning_does_not_delete_table_objects`,
  `row_pruning_does_not_quarantine_table_objects`,
  `row_pruning_does_not_purge_objects`,
  `row_pruning_does_not_prune_snapshots`,
  `row_pruning_does_not_truncate_wal`,
  `row_pruning_does_not_persist_flush_watermark`,
  `row_pruning_does_not_publish_database_manifest_directly`,
  `row_pruning_code_and_fixture_names_do_not_use_milestone_labels`.

### Follow-Up Fix Pass — New And Strengthened Tests

- `row_pruning_proof_visible_version_below_branch_state_rejects` —
  strict lied-low rejection.
- `row_pruning_proof_recovery_health_unknown_rejects` — typed
  recovery-health attestation gate.
- `max_versions_zero_with_floor_above_data_keeps_only_below_floor_survivor`
  — documents the actual `max_versions = Some(0)` behaviour when the
  floor is above all data.
- `shared_table_identity_reachability_blocks_pruning` — now a real
  test that exercises both the proof-level attestation gate and the
  registry-derived `derive_shared_table_safety` helper.
- `pruning_after_materialization_uses_source_identity_not_layer_index`
  — now a real test: snapshots the fingerprint pre/post materialization
  and verifies a stale pre-materialization proof is rejected against
  the post-materialization state.
- `materialized_replacement_tombstone_safety_is_checked` — now a real
  test that fork-materializes a child carrying a parent value plus a
  child-local tombstone and verifies tombstone elision is rejected for
  the resurrection risk.
- `materialized_layer_replacement_preserves_pruned_history_boundary`
  — now a real test exercising materialize → prune → assert
  `InsufficientTimestampHistory` below the retained floor.
- `materialization_with_pruning_preserves_child_local_precedence` —
  now exercises a real materialize → install sentinel → compact L0
  pruning flow on the child, asserting child-local precedence and
  narrowed timestamp coverage.
- `ttl_pruning_preserves_child_newer_override` — now actually runs
  TTL pruning on the child and asserts the inherited-layer gate
  rejects the request while the child's read-view continues to surface
  the override.
- `manifest_records_retained_version_floor` — decodes the
  `storage.retained_history` extension and asserts the embedded
  retained version + timestamp floors match the proof.
- `manifest_missing_pruning_facts_rejects_recovery` — reopens after
  pruning, asserts the extension is present, and asserts the
  recovered branch's `BranchTimestampCoverage` matches the narrowed
  floor (covering the checkpoint-priority recovery path).
- Selected typed-code assertions via the new
  `assert_invalid_compaction_code` helper:
  `row_pruning_request_without_proof_rejects`,
  `row_pruning_proof_branch_mismatch_rejects`,
  `row_pruning_proof_degraded_recovery_rejects`,
  `row_pruning_proof_retained_floor_above_visible_rejects`,
  `row_pruning_proof_visible_version_below_branch_state_rejects`,
  `row_pruning_proof_recovery_health_unknown_rejects`.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Accept pruning without proof | `crates/storage-next/src/branch/state.rs` | Skip missing-proof rejection for pruning policies | `row_pruning_request_without_proof_rejects` |
| Ignore proof freshness | `crates/storage-next/src/branch/pruning.rs` | Accept mismatched branch-state fingerprint | `row_pruning_proof_stale_epoch_rejects_without_mutation` |
| Drop all below-floor values | `crates/storage-next/src/branch/pruning.rs` | Remove below-floor survivor rule | `version_pruning_keeps_retained_rows_and_floor_survivor` |
| Resurrect tombstoned value | `crates/storage-next/src/branch/pruning.rs` | Skip tombstone resurrection-risk scan | `tombstone_pruning_rejects_resurrection_risk` |
| Let max-version pruning evict a required tombstone | `crates/storage-next/src/branch/pruning.rs` | Count required tombstones against the max-version value cap | `max_versions_counts_values_but_not_required_tombstones` |
| Drop TTL row above retained floor | `crates/storage-next/src/branch/pruning.rs` | Ignore version/timestamp floor before TTL elision | `expired_ttl_above_version_floor_is_kept` |
| Treat pruned `as_of` history as absent | `crates/storage-next/src/branch/state.rs` | Skip timestamp coverage narrowing after dropped rows | `version_pruning_as_of_below_floor_returns_insufficient_history` |
| Ignore inherited-layer safety | `crates/storage-next/src/branch/pruning.rs` | Accept `NoReadableInheritedLayers` while inherited layers are attached | `ttl_pruning_across_inherited_parent_child_keeps_required_parent_row` |
| Use ambient time for TTL | `crates/storage-next/src/branch/pruning.rs` | Replace supplied TTL cutoff with wall clock | `row_pruning_does_not_use_wall_clock_or_product_policy` |
| Accept shared-table proof without attestation | `crates/storage-next/src/branch/pruning.rs` | Skip `validate_shared_table_safety` | `shared_table_identity_reachability_blocks_pruning` |
| Accept proof without recovery-health attestation | `crates/storage-next/src/branch/pruning.rs` | Skip `validate_recovery_health` | `row_pruning_proof_recovery_health_unknown_rejects` |
| Lie low about visible version | `crates/storage-next/src/branch/pruning.rs` | Remove the `visible_version < actual_visible` check | `row_pruning_proof_visible_version_below_branch_state_rejects` |
| Forget materialization source in fingerprint | `crates/storage-next/src/branch/pruning.rs` | Stop hashing `materialization_source` on owned tables | `pruning_after_materialization_uses_source_identity_not_layer_index` |
| Drop retained-history extension on rewrite | `crates/storage-next/src/lifecycle/table_manifest.rs` | Skip the `RetainedHistoryFacts::to_extension_section` push in `build_manifest` | `manifest_records_retained_version_floor` |
| Forget retained-history coverage on reopen with checkpoint | `crates/storage-next/src/lifecycle/recovery.rs` | Skip the staged-coverage propagation when checkpoint-priority recovery wins | `manifest_missing_pruning_facts_rejects_recovery` |
| Use string-comparison instead of typed code | `crates/storage-next/src/branch/error.rs` | Change `BranchCompactionInvalidity::ProofMissing.code()` return value | `row_pruning_request_without_proof_rejects` |

### Verification

Commands run for the implementation pass (all passed):

```bash
cargo fmt --package strata-storage-next --check
cargo test -p strata-storage-next --locked --lib table::tests::compaction
cargo test -p strata-storage-next --locked --lib branch::tests::owned_compaction
cargo test -p strata-storage-next --locked --lib branch::tests::inheritance_materialization
cargo test -p strata-storage-next --locked --lib branch::tests::row_pruning
cargo test -p strata-storage-next --locked --lib lifecycle::tests::compaction
cargo test -p strata-storage-next --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_maintenance
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8Y - Branch Lifecycle Completeness

### Shipped Files

- `crates/storage-next/src/lifecycle/branch_lifecycle.rs`
- `crates/storage-next/src/lifecycle/tests/branch_lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/branch_lifecycle/catalog.rs`
- `crates/storage-next/src/lifecycle/tests/branch_lifecycle/clear_delete.rs`
- `crates/storage-next/src/lifecycle/tests/branch_lifecycle/fork.rs`
- `crates/storage-next/src/lifecycle/tests/branch_lifecycle/isolation.rs`
- `crates/storage-next/src/testkit/lifecycle/branch_lifecycle.rs`
- `crates/storage-next/tests/lifecycle_branch_lifecycle.rs`
- `crates/storage-next/tests/lifecycle_source_guard.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/error.rs`
- `crates/storage-next/src/lifecycle/mod.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/testkit/lifecycle/mod.rs`
- `crates/storage-next/src/testkit/mod.rs`

### Implementation Notes

- Added a storage-internal lifecycle branch catalog that keeps branch
  descriptors, branch-local state, and the commit branch registry coherent.
- Added descriptor/status/outcome vocabulary for create, fork, clear, and
  delete paths. The status vocabulary covers active, clearing, deleting, and
  deleted states used by the synchronous catalog implementation.
- Added storage-internal create/list/lookup helpers with duplicate-branch,
  missing-branch, generation-mismatch, generation-exhaustion, and branch-state
  mismatch errors.
- Added current-state fork over the lower-layer inherited-layer API. The fork
  path rejects unflushed active/frozen source rows through the existing branch
  runtime invariant rather than silently dropping them.
- Added fork-at-retained-version support by snapshotting rows visible at the
  requested version, rewriting them to the destination branch id, and installing
  them atomically into the destination branch state.
- Added clear and delete operations that preserve pinned read-view
  reachability. Release planning excludes the removed branch's replacement
  empty state from the post-removal aggregate, while retained pinned snapshots
  continue to protect referenced table identities.
- Added branch lifecycle error codes using the repository error-code format.
- Added `BranchLocalState::fork_snapshot_rows` so fork-at-retained-version can
  snapshot both owned rows and inherited-layer rows visible at a requested
  commit version without requiring eager materialization.
- Wired cache and durable local runtimes to maintain a lifecycle branch catalog
  mirror after commits, flushes, compactions, materializations, and maintenance
  transitions that mutate branch state.
- Split branch lifecycle tests into catalog, fork, clear/delete, and isolation
  modules to keep each source file under the 1,000-line navigation budget.
- Added a testkit branch-lifecycle contract and integration smoke tests for
  catalog, fork-at-version, clear/delete, pinned-view retention, generation, and
  stale-work behavior.

### Tests Added

- `branch_catalog_create_empty_branch`
- `branch_catalog_duplicate_create_rejects`
- `branch_catalog_create_rejects_zero_generation`
- `branch_catalog_list_active_branches_in_deterministic_order`
- `branch_catalog_list_includes_deleted_when_requested`
- `branch_catalog_commit_registry_stays_coherent`
- `branch_catalog_missing_lookup_rejects`
- `branch_catalog_descriptor_branch_mismatch_rejects`
- `branch_catalog_create_does_not_publish_table_objects`
- `branch_catalog_cache_create_reports_no_durable_claim`
- `branch_catalog_runtime_syncs_after_cache_commit`
- `branch_catalog_runtime_syncs_after_durable_commit`
- `cache_branch_lifecycle_after_close_rejects`
- `cache_branch_lifecycle_while_closing_rejects`
- `clear_branch_new_view_empty_and_pinned_view_keeps_old_rows`
- `clear_branch_keeps_branch_id_and_generation_active`
- `clear_branch_rejects_missing_branch`
- `clear_branch_rejects_deleted_branch`
- `clear_branch_removes_active_frozen_owned_and_inherited_rows`
- `clear_branch_after_clear_accepts_new_commits`
- `clear_branch_stale_flush_output_cannot_resurrect_rows`
- `delete_branch_marks_deleted_and_recreate_requires_greater_generation`
- `delete_branch_missing_branch_rejects`
- `delete_branch_already_deleted_is_typed`
- `delete_branch_commit_rejects_after_deleted`
- `delete_branch_new_read_rejects_after_deleted`
- `delete_branch_pinned_view_can_still_read_old_rows`
- `delete_branch_with_shared_parent_table_keeps_parent_readable`
- `recreate_deleted_branch_rejects_generation_exhaustion`
- `recreate_deleted_branch_rejects_same_generation`
- `recreate_deleted_branch_rejects_lower_generation`
- `stale_commit_generation_rejects_after_recreate`
- `stale_flush_task_generation_rejects_after_recreate`
- `stale_compaction_task_generation_rejects_after_recreate`
- `stale_materialization_task_generation_rejects_after_recreate`
- `new_generation_does_not_see_old_rows`
- `fork_current_inherits_owned_tables_without_copying_objects`
- `fork_current_missing_source_rejects_before_destination_mutation`
- `fork_current_existing_destination_rejects`
- `fork_current_nonempty_destination_rejects`
- `fork_current_source_with_active_rows_is_rejected_explicitly`
- `fork_current_inherited_rows_are_visible_in_child`
- `fork_current_child_local_row_shadows_inherited_row`
- `fork_current_source_later_write_does_not_change_child_view`
- `fork_current_records_source_branch_and_fork_version`
- `fork_current_reachability_facts_include_shared_tables`
- `fork_current_works_from_materialized_replacement_tables`
- `fork_current_preserves_inherited_chain_order`
- `fork_at_history_child_excludes_rows_after_requested_version`
- `fork_at_history_child_includes_rows_at_requested_version`
- `fork_at_history_retained_version_succeeds`
- `fork_at_history_visible_latest_matches_current_fork`
- `fork_at_history_after_visible_version_rejects`
- `fork_at_history_below_retained_floor_rejects`
- `fork_at_history_from_inherited_source_includes_visible_parent_row`
- `fork_at_history_tombstone_at_boundary_is_preserved`
- `fork_at_history_source_deleted_before_capture_rejects`
- `fork_at_history_destination_generation_guard_is_enforced`
- `pinned_reachability_protects_removed_tables_from_release`
- `pinned_view_survives_recreate_same_branch_id_new_generation`
- `pinned_view_release_unblocks_retention_candidate`
- `repeated_pinned_reachability_for_same_branch_is_deduped`
- `commit_to_branch_a_does_not_change_branch_b`
- `clear_branch_a_does_not_change_branch_b`
- `delete_branch_a_does_not_change_branch_b`
- `fork_branch_a_to_branch_c_does_not_change_branch_b`
- `row_with_wrong_branch_id_rejects_install`
- `prefix_scan_branch_a_does_not_emit_branch_b_rows`
- `lifecycle_branch_lifecycle_cache_smoke`
- `lifecycle_branch_lifecycle_fork_at_history`
- `lifecycle_branch_lifecycle_clear_delete_no_resurrection`
- `lifecycle_branch_lifecycle_pinned_view_retention`
- `lifecycle_branch_lifecycle_source_stays_storage_internal`

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Accept duplicate create | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Skip existing descriptor check in `create_branch` | `branch_catalog_duplicate_create_rejects` |
| Drop source rows during current fork | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Construct an empty child instead of calling `fork_into_empty_child` | `fork_current_inherits_owned_tables_without_copying_objects` |
| Allow fork from unflushed source rows | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Ignore the lower-layer fork error and install an empty child | `fork_current_source_with_active_rows_is_rejected_explicitly` |
| Include rows newer than fork-at-history version | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Snapshot rows at latest rather than requested fork version | `fork_at_history_child_excludes_rows_after_requested_version` |
| Ignore retained-history floor | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Remove the retained-floor check | `fork_at_history_below_retained_floor_rejects` |
| Lose pinned reachability on clear | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Build release aggregate without pinned snapshots | `pinned_reachability_protects_removed_tables_from_release` |
| Double-count repeated pins | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Feed repeated same-branch pinned snapshots directly into the aggregate | `repeated_pinned_reachability_for_same_branch_is_deduped` |
| Reuse deleted branch generation without increment | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Change recreate comparison from greater-than to greater-or-equal | `delete_branch_marks_deleted_and_recreate_requires_greater_generation` |
| Skip stale descriptor check | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Replace state using a queued descriptor after clear/delete/recreate | `clear_branch_stale_compaction_output_cannot_resurrect_rows` / `stale_flush_task_generation_rejects_after_recreate` |
| Lose inherited-layer rows during fork-at-version | `crates/storage-next/src/branch/state.rs` | Omit inherited-layer rows from `fork_snapshot_rows` | `fork_at_history_from_inherited_source_includes_visible_parent_row` |
| Treat inherited table owner as source branch | `crates/storage-next/src/branch/facts.rs` | Collapse owner branch and source branch in inherited table refs | `fork_current_reachability_facts_include_shared_tables` |
| Cross-branch read leakage | `crates/storage-next/src/lifecycle/branch_lifecycle.rs` | Return the wrong branch state from catalog lookup | `commit_to_branch_a_does_not_change_branch_b` |

### Deferred To Remaining Branch-Lifecycle Work

- Durable branch catalog/table-manifest publication and recovery of multi-branch
  create/fork/clear/delete descriptors. The current durable runtime mirrors the
  active branch into a catalog, but the persisted database manifest still owns
  only the single active branch path.
- Public branch lifecycle API wrappers and product-facing branch naming stay in
  the public storage API boundary.
- Timeline-based timestamp fork lookup is not exposed by the branch catalog yet;
  current fork-at-version tests cover explicit retained commit versions.
- Full generated model, fault-window tests, and fuzz targets over branch
  lifecycle scripts remain future assurance work; the shipped testkit contract is
  a deterministic integration smoke, not a byte-decoded reference model.
- Durable recovery round trips for branch delete/recreate catalog state are
  blocked on the durable multi-branch catalog format.

### Verification

Commands run for this implementation pass (all passed):

```bash
cargo fmt --package strata-storage-next
cargo test -p strata-storage-next --locked --lib lifecycle::tests::branch_lifecycle
cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle
cargo test -p strata-storage-next --locked --test lifecycle_source_guard
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
git diff --check
```

## L8Z - Commit Hardening And Pre-L9 Readiness

### Source Plans

- `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`

### Shipped Files

- `crates/storage-next/src/lifecycle/config.rs`
- `crates/storage-next/src/lifecycle/wal_growth.rs`
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs`
- `crates/storage-next/src/lifecycle/durable/maintenance.rs`
- `crates/storage-next/src/lifecycle/cache.rs`
- `crates/storage-next/src/service/wal.rs`
- `crates/storage-next/src/lifecycle/tests/commit_hardening.rs`
- `crates/storage-next/src/lifecycle/tests/mod.rs`
- `crates/storage-next/src/lifecycle/tests/checkpoint/shared.rs`

### Preserved Storage Vocabulary

- Commit version remains the only V1 durable ordering identity. The existing
  transaction-id source guards continue to reject transaction-id fields,
  allocators, and user-facing transaction wording in storage code.
- Durable-local runtime now evaluates a deterministic WAL-growth policy after
  successful durable commits. The policy emits raw storage facts:
  retained WAL segments, retained WAL bytes, active segment id/size, dirty
  bytes, dirty records, commits since checkpoint, trigger kind, enqueue fact,
  and any deferred health/source error.
- Cache mode reports `NoDurableAction` for WAL-growth policy evaluation. It
  does not enqueue checkpoint, WAL retention, or durable maintenance work.
- The automatic policy only enqueues checkpoint work through the existing
  deterministic maintenance executor. It does not spawn background work and
  does not truncate WAL unless existing checkpoint/table-manifest proof later
  authorizes truncation.

### Retired From V1

- Public transaction sessions.
- Durable transaction ids.
- Distributed/global commit version allocation.
- Cross-branch atomic product transactions.
- Rich/background/adaptive checkpoint scheduling.
- Physical format freeze and compatibility policy. That remains owned by L10.

### Tests Added

- `wal_growth_policy_triggers_on_each_threshold_deterministically`
- `automatic_checkpoint_triggers_when_wal_bytes_exceed_threshold`
- `automatic_checkpoint_triggers_when_retained_segments_exceed_threshold`
- `automatic_checkpoint_does_not_trigger_below_threshold`
- `automatic_checkpoint_uses_existing_maintenance_executor`
- `automatic_checkpoint_coalesces_existing_checkpoint_task`
- `automatic_checkpoint_deferred_while_quiesce_active`
- `automatic_checkpoint_deferred_while_close_in_progress`
- `automatic_checkpoint_deferred_while_recovery_in_progress`
- `automatic_checkpoint_failure_records_health_debt`
- `automatic_checkpoint_cache_mode_reports_no_durable_action`
- `automatic_checkpoint_disable_requires_explicit_config`
- `automatic_checkpoint_does_not_truncate_wal_without_retention_proof`
- `automatic_checkpoint_truncates_wal_only_after_checkpoint_or_table_manifest_proof`
- `wal_growth_pressure_facts_are_visible_to_public_boundary`
- `automatic_checkpoint_policy_is_deterministic_without_background_thread`
- Config validation assertions for the default and invalid WAL-growth policy
  thresholds.

### Sensitivity Probes Recorded

| Probe | Mutated file/line | Mutation | Expected failing test |
|---|---|---|---|
| Disable byte-pressure trigger | `crates/storage-next/src/lifecycle/wal_growth.rs` | Remove retained-byte comparison from `trigger_for` | `automatic_checkpoint_triggers_when_wal_bytes_exceed_threshold` |
| Disable segment-pressure trigger | `crates/storage-next/src/lifecycle/wal_growth.rs` | Remove retained-segment comparison from `trigger_for` | `automatic_checkpoint_triggers_when_retained_segments_exceed_threshold` |
| Trigger below threshold | `crates/storage-next/src/lifecycle/wal_growth.rs` | Change comparison to `>=` or ignore thresholds | `automatic_checkpoint_does_not_trigger_below_threshold` |
| Skip automatic post-commit evaluation | `crates/storage-next/src/lifecycle/durable/bootstrap.rs` | Remove policy evaluation after successful durable commit | `automatic_checkpoint_triggers_when_wal_bytes_exceed_threshold` |
| Bypass maintenance executor | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Run checkpoint directly instead of enqueueing a task | `automatic_checkpoint_uses_existing_maintenance_executor` / `automatic_checkpoint_coalesces_existing_checkpoint_task` |
| Ignore quiesce admission | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Remove quiesce check before enqueue | `automatic_checkpoint_deferred_while_quiesce_active` |
| Ignore close/recovery admission | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Remove lifecycle admission check before enqueue | `automatic_checkpoint_deferred_while_close_in_progress` / `automatic_checkpoint_deferred_while_recovery_in_progress` |
| Drop policy failure health debt | `crates/storage-next/src/lifecycle/durable/maintenance.rs` | Return lower-layer errors instead of deferred outcome | `automatic_checkpoint_failure_records_health_debt` |
| Mark ordinary deferral as health debt | `crates/storage-next/src/lifecycle/wal_growth.rs` | Attach recovery health to admission-only deferrals | `automatic_checkpoint_deferred_while_quiesce_active` / `automatic_checkpoint_deferred_while_close_in_progress` |
| Truncate WAL from policy checkpoint | `crates/storage-next/src/lifecycle/wal_growth.rs` | Build checkpoint task with WAL truncation enabled | `automatic_checkpoint_does_not_truncate_wal_without_retention_proof` |
| Lose proof-gated truncation path | `crates/storage-next/src/lifecycle/checkpoint.rs` | Ignore retention proof on explicit checkpoint/truncation request | `automatic_checkpoint_truncates_wal_only_after_checkpoint_or_table_manifest_proof` |
| Claim durable action in cache mode | `crates/storage-next/src/lifecycle/cache.rs` | Return checkpoint/enqueue status from cache evaluation | `automatic_checkpoint_cache_mode_reports_no_durable_action` |
| Allow zero enabled thresholds | `crates/storage-next/src/lifecycle/config.rs` | Remove enabled-policy threshold validation | `lifecycle_config_rejects_zero_limits` |

### Verification

Commands run for this implementation pass:

| Command | Result |
|---|---|
| `cargo fmt --package strata-storage-next` | PASS |
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --locked --lib lifecycle::tests::commit_hardening` | PASS, 16 tests |
| `cargo test -p strata-storage-next --locked --lib lifecycle::tests` | PASS, 958 tests |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS, 96 tests |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 1 - Plan Corrections (docs only)

Date: 2026-05-27. No code change. Audit findings from
`docs/architecture/implementation-plans/M4/L8/l8z-audit-and-followup.md`
applied to both L8Z planning docs and this porting log.

| Edit | Target file | Section | Change |
|---|---|---|---|
| 1 | impl plan | §"Minimal Automatic Checkpoint And WAL-Growth Policy" | Added "Status: shipped" header sentence pointing at `wal_growth.rs`, `maintenance.rs::evaluate_wal_growth_policy`, `cache.rs::evaluate_wal_growth_policy`, and `lifecycle/tests/commit_hardening.rs`. |
| 1 | impl plan | §"Implementation Steps" | Removed step 11 (WAL-growth trigger — already shipped). Renumbered 12-14 to 11-13 with a parenthetical at the renumber point. |
| 1 | test plan | §11 | Prepended "Verifies shipped behavior; not gating new implementation work" header. |
| 2 | impl plan | §"Durable Gate Hardening" | Replaced "Two acceptable designs" with the committed single-admission design. Rewrote rule 1 as a structural-property assertion citing `commit/tests/durable.rs:1290`. Added rule 1b documenting the load-bearing sequential same-branch mismatch path at `durable_gate.rs:266-268`. |
| 3 | impl plan | §"Branch Generation Guard Coverage" rule 4 | Split single rule into two: "Deleted lifecycle branches reject" and "`CommitBranchState::Deleting` is transient inside `delete_branch`". Aligns with C Phase 1's removal of `LifecycleBranchStatus::Deleting`. |
| 3 | test plan | §2 Assertions | Same two-clause split. |
| 4 | test plan | §"Test Locations" item 15 | Corrected `src/testkit/lifecycle/commit_hardening.rs` to `src/lifecycle/tests/commit_hardening.rs`. |
| 5 | test plan | §1 #5, §2 #1, §2 #4, §3 block, §7 #4 | Annotated each duplicate-name test with an italic "Existing: ..." pointer to its shipped counterpart. |
| 6 | test plan | §5 #7 | Renamed `recovery_replay_runs_under_exclusive_open_or_quiesce` to `recovery_replay_runs_under_exclusive_open`. |
| 6 | impl plan | §"Quiesce Integration" required users | Dropped item 4 ("recovery bootstrap and replay"). Renumbered. Added an exclusive-open rationale paragraph citing `LifecycleStateMachine::admit`. |
| 7 | test plan | §11 #13 | Renamed `wal_growth_pressure_facts_are_visible_to_public_boundary` to `wal_growth_pressure_facts_have_stable_observation_api`. The live test in `commit_hardening.rs:375` keeps its current name; the test plan name is forward-looking. |
| 8 | test plan | §11 Assertions | Added prose clarifying that tests #5, #6, and #7 verify state-machine-driven deferral via `LifecycleStateMachine::admit`, not concurrent execution. |
| 9 | impl plan | §"Durable Gate Hardening" | Added "Cache Mode Participation" subclause documenting that cache-mode commits acquire the global admission lock via `commit/cache.rs:77`. |
| 10 | impl plan | new §"Open Questions" between §"Deferred" and §"Exit Gate" | Added three entries: (A) recovery quiesce locked to exclusive-open; (B) fork timeline inheritance deferred to Phase 6; (C) WAL-record generation field deferred to Phase 5 with catalog-derived default. |

Decision locks recorded in this pass:

- Test plan §5 item 7 resolves to `recovery_replay_runs_under_exclusive_open`. Phase 4 does not wire quiesce into `lifecycle/durable/bootstrap.rs`.
- The durable gate ships as single-admission; keyed multi-entry tracking is deferred.

Phase 1 introduces no new tests and no code changes. The 11 edits cap with
the verification matrix below.

#### Verification

| Check | Expected | Result |
|---|---|---|
| `grep -n "testkit/lifecycle/commit_hardening" docs/architecture/implementation-plans/M4/L8/l8z-*.md` | zero hits | PASS |
| `grep -n "two acceptable designs" docs/architecture/implementation-plans/M4/L8/l8z-*.md` | zero hits | PASS |
| `grep -n "Deleted and deleting branches" docs/architecture/implementation-plans/M4/L8/l8z-*.md` | zero hits | PASS |
| `grep -n "recovery_replay_runs_under_exclusive_open_or_quiesce" docs/architecture/implementation-plans/M4/L8/l8z-*test-plan.md` | zero hits | PASS |
| `grep -n "^## Open Questions" docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md` | one hit | PASS |
| `grep -n "Cache Mode Participation" docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md` | one hit | PASS |
| `grep -n "Status: shipped" docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-implementation-plan.md` | one hit | PASS |
| `grep -n "lifecycle/tests/commit_hardening" docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md` | at least one hit | PASS |
| `grep -n "recovery_replay_runs_under_exclusive_open\b" docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md` | one hit | PASS |

### L8Z Phase 2 - Milestone-Label Sweep + Source-Guard Widening

Date: 2026-05-27. The Pre-L9 surface-readiness rule mandates that V1
slice labels (`L4`-`L9`, `L7M`/`L8Y`/`L8Z`-style slice codes, `M0`-`M9`
milestones, `M3B2`-style milestone slices) must not appear in source,
test names, fixture bytes, or user-facing error strings. Until this
phase, the rule was enforced only inside `src/lifecycle/`. The audit
found 13 prose comments, 6 test names, 2 closeout docstrings, and one
slice-mapping multiline comment carrying labels outside that scope.

#### Sweep

| Site | Change |
|---|---|
| `src/commit/{guard,replay,conflict,cache,durable}.rs`, `src/branch/read.rs`, `src/testkit/{commit_runtime,commit_runtime_runner,commit_runtime_script}.rs`, `src/format/{mod,table/mod}.rs` | 13 prose comments rephrased: dropped "L6 apply", "L8 owns", "L7M scripts", "L5 assigns" wording while preserving architectural meaning. |
| `src/commit/tests/conflict.rs`, `src/branch/tests/identity_state.rs`, `src/testkit/branch_lsm/contracts.rs` | 3 additional prose sites the audit missed (test-message strings + a testkit docstring). |
| `tests/commit_runtime_closeout.rs`, `tests/table_runtime_closeout.rs` | 2 closeout docstrings rephrased: dropped "L7" / "M4-L5" prefix. |
| `tests/branch_lsm_source_guard.rs:145-158` | Slice-mapping multiline comment rephrased from `L6C - commit append, L6D - read-view pinning, ...` to behavior names without slice codes. Assertion message at line 158 updated. |
| `src/commit/tests/batch.rs:73`, `src/commit/tests/cache.rs:589`, `src/commit/tests/durable.rs:407`, `src/branch/tests/owned_compaction.rs:2108`, `src/table/tests/cache.rs:676`, `tests/branch_lsm_source_guard.rs:287` | 6 test functions renamed to drop slice labels: `_all_l7b_modes` -> `_all_durability_modes`; `cache_commit_l6_apply_failure_*` -> `cache_commit_apply_failure_*`; `_before_l6_apply` -> `_before_apply_phase`; `branch_compaction_l5_build_failure_*` -> `branch_compaction_build_failure_*`; `_preserves_l5_key_boundaries` -> `_preserves_key_boundaries`; `_allows_owned_l6_entrypoints` -> `_allows_owned_entrypoints`. |
| `tests/commit_runtime_closeout.rs:134,187` | Updated inventory strings to match renamed tests. |
| `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md:1461`, `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md:1435` | Updated sensitivity-probe ledger references to renamed tests. |

#### Widen

| Artifact | Detail |
|---|---|
| `crates/storage-next/tests/common/source_guard_helpers.rs` (new) | Shared helper module exporting `contains_milestone_label` (tighter pattern than the previous `contains_architecture_label`: catches `L[4-9]`, `M[0-9]`, and slice codes; allows LSM-level `L[0-3]` references and PascalCase symbol names via a 4-char lookahead for lowercase) plus `collect_rs_files` and `collect_rs_files_including_tests`. |
| `tests/lifecycle_source_guard.rs::lifecycle_implementation_avoids_architecture_labels` | Refactored to call `common::source_guard_helpers::contains_milestone_label`. Inline byte-pair helper removed. |
| `tests/commit_runtime_source_guard.rs::commit_runtime_implementation_avoids_architecture_labels` | New. Scans `src/commit/`, `src/testkit/commit_runtime*.rs`, `tests/commit_runtime_*.rs`. |
| `tests/branch_lsm_source_guard.rs::branch_lsm_implementation_avoids_architecture_labels` | New. Scans `src/branch/`, `src/testkit/branch_lsm/`, `tests/branch_lsm_*.rs`. |
| `tests/lifecycle_closeout.rs::closeout_files_avoid_architecture_labels` | New. Scans the four closeout integration-test files. |

#### Test-plan edit

`docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`
§"Source Guards" item 4 reframed: dropped "fuzz corpora" (architecturally
noisy; corpus seeds are random bytes whose label-shaped substrings carry
no semantic meaning) and clarified the error-string clause as caught via
source scans of `format!` templates and `#[error(...)]` attributes. Item
7 dropped "corpora" from the scan scope.

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS, 96 tests |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS, 12 tests |
| `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` | PASS, 10 tests |
| `cargo test -p strata-storage-next --locked --test table_runtime_source_guard` | PASS, 15 tests |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS, 11 tests |
| `cargo test -p strata-storage-next --features testkit --locked --lib` | PASS, 2358 tests |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 3 - Durable Gate Consolidation

Date: 2026-05-28. Phase 3 was scoped as durable gate consolidation +
close-clean enforcement + cache-mode test coverage. Plan-mode
exploration found that four of the audit's five Phase 3 items were
already shipped or covered by existing tests under different names;
the fifth (sequential same-branch error split) was overridden by Phase
1's impl-plan rewrite. The remaining work was a single source guard.

#### Disposition of Audit Phase 3 Items

| Audit Item | Disposition |
|---|---|
| Close rejects clean-state report when `durable_gate.unresolved.is_some()` | **Shipped**: `src/lifecycle/durable/close.rs:164-175` rejects with `LifecycleError::CloseFailed`. Test `durable_close_does_not_report_complete_with_unresolved_durable_gate` at `src/lifecycle/tests/durable.rs:675`. Audit doc cited stale line numbers `638-642`. |
| `cross_branch_second_admission_blocks_at_active_admission` (working title) | **Shipped**: `durable_active_global_admission_blocks_other_branch_before_wal_append` at `src/commit/tests/durable.rs:1290` already asserts the structural property (second branch rejected before `record_unresolved`). |
| `cache_commit_observes_global_durable_admission_lock` (working title) | **Shipped**: `cache_commit_rejects_any_unresolved_durable_gate_before_allocation` at `src/commit/tests/cache.rs:935` exercises cache mode acquiring the global lock and being blocked by a pre-recorded unresolved fact. |
| `cache_record_unresolved_uses_not_durable_class` (working title) | **Shipped**: `cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard` at `src/commit/tests/cache.rs:641` asserts `CommitDurabilityClass::NotDurable` on the recorded gate entry. |
| Split generic same-branch `record_unresolved` mismatch error | **Overridden by Phase 1**: impl-plan §"Durable Gate Hardening" rule 1b explicitly keeps the generic error code because existing tests (`commit/tests/durable_gate.rs:369-405`) depend on it. |
| Restrict `mark_deleting` to `delete_branch` | **Implemented via source guard** (visibility tightening would have broken the cross-module call from `lifecycle::branch_lifecycle::delete_branch` into `commit::branch_registry::mark_deleting`). |

#### Artifact Added

`tests/commit_runtime_source_guard.rs::mark_deleting_is_only_called_from_delete_branch`
scans `src/commit/`, `src/branch/`, and `src/lifecycle/` for every
`mark_deleting(` occurrence. Allowed call sites: the definition in
`src/commit/branch_registry.rs`, and calls inside the
`fn delete_branch(` body in `src/lifecycle/branch_lifecycle.rs`
(detected by brace-matched function-body byte-range analysis). Two
sibling helper-validation sub-tests (`_classifier_accepts_*` and
`_classifier_rejects_*`) exercise the call classifier against
synthetic source fragments.

#### Test-plan edit

`docs/architecture/implementation-plans/M4/L8/l8z-commit-hardening-pre-l9-readiness-test-plan.md`
§7 items 3 and 4 re-annotated as structurally unreachable, with
cross-references to the shipped `durable_active_global_admission_blocks_other_branch_before_wal_append`
test. Item 9 (`durable_gate_close_requires_clean_state`) annotated
with the shipped `close.rs:164-175` reference. New "Cache Mode (Phase
3 verification)" subsection added listing the two shipped cache-mode
tests by their actual names.

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS, 15 tests (12 existing + 3 new) |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS, 96 tests |
| `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` | PASS, 10 tests |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::cache` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::durable` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests::durable` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 4 - Quiesce Wire-up for Branch Lifecycle

Date: 2026-05-28. The L8Z impl-plan's §"Quiesce Integration" lists
five required quiesce users — checkpoint, branch fork, branch
clear/delete, durable close, and L9-bound maintenance. Two were
already wired (checkpoint, durable close). Phase 4 wires the
remaining three (clear, delete, fork) in both durable and cache
runtimes so the rule holds uniformly across modes (matching
Phase 1's required-users list).

#### Wrapper Edits (10 sites)

| Wrapper | File:Line | Quiesce holds through |
|---|---|---|
| `LifecycleDurableLocalRuntime::fork_current` | `lifecycle/durable/bootstrap.rs:559` | catalog fork + `publish_branch_catalog` |
| `LifecycleDurableLocalRuntime::fork_at_retained_version` | `lifecycle/durable/bootstrap.rs:578` | catalog fork + `publish_branch_catalog` |
| `LifecycleDurableLocalRuntime::fork_at_retained_timestamp` | `lifecycle/durable/bootstrap.rs:599` | catalog fork + `publish_branch_catalog` |
| `LifecycleDurableLocalRuntime::clear_branch` | `lifecycle/durable/bootstrap.rs:619` | catalog clear + health-debt push + `publish_branch_catalog` + `publish_pending_releases` |
| `LifecycleDurableLocalRuntime::delete_branch` | `lifecycle/durable/bootstrap.rs:643` | catalog delete + health-debt push + `publish_branch_catalog` + `publish_pending_releases` |
| `LifecycleCacheRuntime::fork_current` | `lifecycle/cache.rs:273` | catalog fork |
| `LifecycleCacheRuntime::fork_at_retained_version` | `lifecycle/cache.rs:289` | catalog fork |
| `LifecycleCacheRuntime::fork_at_retained_timestamp` | `lifecycle/cache.rs:311` | catalog fork |
| `LifecycleCacheRuntime::clear_branch` | `lifecycle/cache.rs:329` | catalog clear |
| `LifecycleCacheRuntime::delete_branch` | `lifecycle/cache.rs:344` | catalog delete |

All ten use the same RAII pattern: `let _quiesce =
self.guard_set.try_begin_quiesce().map_err(commit_error)?;`. The
catalog (`LifecycleBranchCatalog`) stays unchanged; the wrappers own
the quiesce window. A `#[cfg(test)] guard_set()` accessor was added
to `LifecycleCacheRuntime` (mirroring the existing durable one) so
tests can hold a branch guard while asserting the wrapper rejects.

#### Tests Added (11)

In `src/lifecycle/tests/durable.rs`:

- `durable_clear_branch_requires_quiesce_and_rejects_when_branch_guard_active`
- `durable_delete_branch_requires_quiesce_and_rejects_when_branch_guard_active`
- `durable_fork_current_requires_quiesce_and_rejects_when_branch_guard_active`
- `durable_fork_at_retained_version_requires_quiesce_and_rejects_when_branch_guard_active`
- `durable_fork_at_retained_timestamp_requires_quiesce_and_rejects_when_branch_guard_active`
- `branch_lifecycle_quiesce_guard_releases_on_failure_so_followup_acquire_succeeds`
- `assert_quiesce_unavailable` helper

In `src/lifecycle/tests/cache.rs`:

- `cache_clear_branch_requires_quiesce_and_rejects_when_branch_guard_active`
- `cache_delete_branch_requires_quiesce_and_rejects_when_branch_guard_active`
- `cache_fork_current_requires_quiesce_and_rejects_when_branch_guard_active`
- `cache_fork_at_retained_version_requires_quiesce_and_rejects_when_branch_guard_active`
- `cache_fork_at_retained_timestamp_requires_quiesce_and_rejects_when_branch_guard_active`
- `assert_cache_quiesce_unavailable` helper

Each rejection test acquires a branch guard on the target branch,
calls the wrapper, asserts `LifecycleError::LowerLayer { layer:
CommitRuntime, source: CommitRuntimeError::CommitQuiesceUnavailable
}`, and confirms catalog state is unchanged via
`runtime.list_branches(false)`. The release-on-failure test holds a
guard so the first wrapper call fails on quiesce acquisition, then
drops the guard, acquires a fresh quiesce token directly (proving
RAII Drop ran), and re-invokes the wrapper to confirm it now
succeeds.

#### Test-Plan §5 Inventory

Test plan §5 items 1-11 annotated to reference shipped tests
(checkpoint guard set tests, durable close tests, Phase 4 new
tests) or Phase 1 lock-in (item 7). Item 12 remains aspirational.

#### Disposition Summary

| Required user (impl plan §"Quiesce Integration") | Status |
|---|---|
| checkpoint row capture | **Shipped** (`lifecycle/checkpoint.rs:1434`) |
| branch fork and fork-at-history | **Phase 4** (durable + cache wrappers) |
| branch clear and delete | **Phase 4** (durable + cache wrappers) |
| durable close | **Shipped** (`lifecycle/durable/close.rs:151`) |
| L9-bound maintenance | **Deferred** (L9 is post-V1) |

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests::durable` | PASS, 69 tests (63 existing + 6 new) |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests::cache` | PASS, 31 tests (26 existing + 5 new) |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests::branch_lifecycle` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::guard` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 5 - Branch-Generation Guard Plumbing

Date: 2026-05-28. The L8Z impl-plan's §"Branch Generation Guard
Coverage" lists twelve required surfaces. Plan-mode exploration
found that most of the audit's gaps are addressed structurally by
the existing `CommitBranchGenerationGuard` validation in
`branch_state_mut` and `replace_active_branch_state_with_descriptor`,
combined with Phase 4's quiesce wiring on branch-lifecycle wrappers.

#### Disposition Summary

| Audit Gap | Disposition |
|---|---|
| **Replay generation gate** | **Deferred to a future slice**. Three options considered: (D) recovery-time `created_at` filter without WAL format change, (A) `branch_generation` field added to `WalRecord` with format-version bump, (C) defer. Option D required strict semantics on `created_at` that 15+ existing call sites violate (caller-controlled label, not a strict commit-version bound). Option A and a refined D′ both require format changes of comparable LOC. Deferred so a dedicated future slice picks one path and lands it cleanly. The gap is real: stale pre-recreate WAL records below the live `created_at` are silently applied to live state. Catalog manifest is authoritative for the live generation. |
| **Table-manifest publication generation** | **Already structurally safe**. The helper takes `branch: &BranchLocalState` which is only obtainable via `branch_state_mut(branch_id, Guard::exact(gen))`. The guard fails on stale generation before any `&BranchLocalState` reaches `publish_table_manifest_for_branch_with_budget`. Existing `stale_flush_task_generation_rejects_after_recreate` and `stale_compaction_task_generation_rejects_after_recreate` tests verify the rejection path. |
| **Retention / quarantine generation** | **Already structurally safe**. Same model as table-manifest publication: branch-scoped helpers receive validated `&BranchLocalState` references; the catalog guard rejects stale generations before any retention work begins. |
| **Close-drain per-task generation** | **Already structurally safe** under Phase 4's quiesce wiring. Recreate cannot run during close (close acquires quiesce; recreate fails on quiesce token). The drain's up-front `branch_state_mut(..., Guard::exact(captured_gen))` rejects on mismatch before the runner begins. |
| **`set_parent_for_recovery` exclusivity** | **Phase 5: `RecoveryExclusivityToken`**. Added a `pub(crate)` zero-size token in `lifecycle/branch_lifecycle.rs` whose constructor is `pub(super)` and further constrained by source guard. The bootstrap module is the only minting site. Threaded through both `set_parent_for_recovery` call sites in `lifecycle/durable/bootstrap.rs` (lines 810, 840). |
| **No-generation paths source guard** | **Phase 5: `recovery_exclusivity_token_is_minted_only_in_bootstrap`**. Source-guard test in `tests/lifecycle_source_guard.rs` scans the lifecycle production tree (excluding `lifecycle/branch_lifecycle.rs` definition and `lifecycle/durable/bootstrap.rs` minting site) and rejects any `RecoveryExclusivityToken::new(` occurrence. A future slice may extend the scan to a broader generic guard-free helper inventory. |

#### Artifacts Added

| File | Change |
|---|---|
| `src/lifecycle/branch_lifecycle.rs` | Added `RecoveryExclusivityToken` zero-size type with `pub(super) new()`. Added `lookup_descriptor` accessor on `LifecycleBranchCatalog` (general utility for future generation-aware paths). Updated `set_parent_for_recovery` signature to take a `_token: RecoveryExclusivityToken` parameter. Updated docstring to point at the token. |
| `src/lifecycle/mod.rs` | Re-exported `RecoveryExclusivityToken` from `branch_lifecycle::*`. |
| `src/lifecycle/durable/bootstrap.rs` | Imported `RecoveryExclusivityToken`. Updated both `set_parent_for_recovery` call sites (lines 810, 840) to pass `RecoveryExclusivityToken::new()`. |
| `tests/lifecycle_source_guard.rs` | Added `recovery_exclusivity_token_is_minted_only_in_bootstrap` source guard. |
| `l8z-commit-hardening-pre-l9-readiness-test-plan.md` | Annotated all 12 §2 items with shipped / Phase 5 / deferred dispositions. |

#### Deferred Replay-Safety Gap (documented for future slice)

Scenario:

1. Branch A is active at generation 1.
2. Several commits land via durable WAL → WAL records carry `branch_id = A`, `commit_version` 1..N.
3. Branch A is deleted.
4. Branch A is recreated at generation 2 (next `commit_version` N+1 or higher).
5. Crash before checkpoint (so WAL still has the deleted A's records).
6. Recovery:
   - Catalog manifest replays first → catalog shows Branch A at gen 2 with new `created_at` value.
   - WAL replay loop encounters records 1..N (gen-1 records).
   - The record carries no generation field; the dispatcher uses the live catalog generation (2) as the guard.
   - `validate_recovered_wal_package` accepts the records (branch is "non-Deleted").
   - Gen-1 records get applied into Branch A's gen-2 state. Silent corruption.

The catalog manifest is authoritative for the live generation. The future slice must either add `branch_generation` to `WalRecord` (format change with version bump and ~4 golden vector regenerations) OR refactor `LifecycleBranchDescriptor::created_at` semantics so a recovery-time filter (`record.commit_version < live_descriptor.created_at`) can rely on it.

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests` | PASS, 969 tests |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS, 97 tests (96 existing + 1 new) |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 6 - Timeline + Visibility Edge-Case Pinning

Date: 2026-05-28. The L8Z impl-plan flagged four read-side correctness
gaps in §"Timeline Hardening" and §"Global Visibility Safety". Plan-
mode exploration found three of the four already closed by shipped
code; the fourth (fork timeline inheritance) is correct-but-
unspecified behavior. Phase 6 is a documentation + pinning slice,
not a refactor.

#### Disposition of Audit Gaps

| Audit Gap | Disposition |
|---|---|
| **Timeline-only WAL rejection** | Already closed. `validate_replay_rows` (`commit/replay.rs:312-341`) returns `CommitRuntimeError::InvalidCommitState { reason: "replay payload is missing user mutation rows" }` for timeline-only payloads. Test `replay_rejects_timeline_only_payload_without_user_mutation` covers it. No Phase 6 work needed. |
| **Fork timeline inheritance** | Pinned as Option C. Implementation uses `BranchEffectiveReadBound::for_inherited_layer` to cap as-of reads at `(fork_version, timestamp)`; parent's physical rows are read via inherited layers; per-row `commit_timestamp` drives timestamp matching. No timeline transcription at fork; no centralized parent-timeline lookup at read. Three new pinning tests + a docstring on `for_inherited_layer` lock the contract. Impl plan §"Open Questions" §B closes with the Option C resolution. |
| **Cache RYW under AppliedButNotVisible** | Real but intentional. Same-branch `latest()` reads on the failing branch return the applied row (read-your-writes preservation). Cross-branch leak is closed by the unresolved durable gate. Phase 6 adds one explicit pinning test + a source-level comment on the failure path. |
| **Allocator-vs-uncertain replay** | Closed by construction. Uncertain WAL records are not durable — they do not survive the failure. Existing test `durable_uncertain_wal_failure_is_distinct_and_leaves_no_visible_rows` (`commit/tests/durable.rs:1065`) verifies the precondition (`fixture.wal.records.len() == 0`). Recovery-side property (no WAL → no replay → no phantom row) is structurally implied; no additional recovery test was added because that would require extending `DurableTestBackend` with a new uncertain-WAL-append mode (~150 LOC of fake backend code) for a property that holds by construction. |

#### Artifacts Added

| File | Change |
|---|---|
| `src/lifecycle/tests/branch_lifecycle/fork.rs` | Added three pinning tests: `forked_branch_at_timestamp_before_fork_returns_parent_row`, `forked_branch_at_timestamp_after_fork_returns_child_row`, `forked_branch_isolated_from_parent_post_fork_commits`. |
| `src/branch/read.rs` | Added a 25-line docstring on `BranchEffectiveReadBound::for_inherited_layer` documenting the fork-inheritance contract (Option C). |
| `src/commit/cache.rs` | Added a 10-line comment on the visibility-failure path explaining the same-branch RYW contract and the cross-branch protection via the unresolved gate. |
| `src/commit/tests/cache.rs` | Added `cache_applied_not_visible_row_is_visible_to_same_branch_read_your_writes` pinning test. |
| `l8z-commit-hardening-pre-l9-readiness-implementation-plan.md` | Replaced Open Questions §B deferral with the Option C decision, referencing the three pinning tests and the `for_inherited_layer` docstring. |
| `l8z-commit-hardening-pre-l9-readiness-test-plan.md` | Annotated §6 (Global Visibility Safety) items 1-10 with shipped/Phase 6 references. Annotated §9 (Timeline Hardening) items 1-12 with shipped tests + Phase 6 pinning tests for items 5, 8, 12. |

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests::branch_lifecycle::fork` | PASS, 30 tests (27 existing + 3 new) |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::cache` | PASS (26 existing + 1 new) |
| `cargo test -p strata-storage-next --features testkit --locked --lib lifecycle::tests` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |

### L8Z Phase 7 - Assurance Closeout

Date: 2026-05-28. The L8Z impl-plan §"Implementation Steps" items
11-13 cover the assurance layer: generated/fault/fuzz tests + Q-Z
closeout source guards + porting log records. Plan-mode exploration
found that most plan-listed assurance items are already covered by
existing tests at adjacent layers (`tests/lifecycle_faults.rs` has
19 fault tests; `tests/lifecycle_closeout.rs` has 11 closeout
tests; `tests/commit_runtime_closeout.rs` has 9; existing 4 fuzz
targets cover the audit's hardening intent under different names).

The user chose Path B (Pragmatic): annotate the test plan with
existing-coverage references; add the 4 Q-Z closeout tests that
scan real inventory; defer the 4 most speculative Q-Z tests
(covered indirectly); document the fuzz-target rationale; add the
porting log sensitivity ledger + command matrix; do not add new
fuzz targets.

#### Sensitivity-Probe Ledger

| Probe | Mutation Site | Mutation Description | Fired Test | Notes |
|---|---|---|---|---|
| S1 | `format/wal.rs::WalRecord` | Add a `transaction_id: u64` field | `commit_runtime_source_guard_catches_product_vocabulary`, `wal_format_source_does_not_use_legacy_payload_vocabulary` | Locks the V1 commit-version-as-ordering-identity rule. |
| S2 | `commit/cache.rs::execute` | Skip `admit_mutating_commit` generation validation | `cache_commit_rejects_missing_deleted_and_stale_generation_before_allocation` | Stale-generation rejection at cache mode. |
| S3 | `commit/durable.rs::execute` | Skip `admit_mutating_commit` generation validation | `stale_commit_generation_rejects_after_recreate` | Stale-generation rejection at durable mode. |
| S4 | `lifecycle/durable/maintenance.rs` (flush task path) | Drop the captured-generation validation on flush task | `stale_flush_task_generation_rejects_after_recreate` | Catalog-level guard rejects via `BranchNotWritable`. |
| S5 | `lifecycle/durable/maintenance.rs` (compaction task path) | Drop the captured-generation validation on compaction task | `stale_compaction_task_generation_rejects_after_recreate` | Same `BranchNotWritable` rejection path. |
| S6 | `lifecycle/durable/maintenance.rs` (materialization task path) | Drop the captured-generation validation on materialization task | `stale_materialization_task_generation_rejects_after_recreate` | Same rejection path. |
| S7 | `commit/replay.rs::validate_replay_rows` | Accept a timeline-only payload | `replay_rejects_timeline_only_payload_without_user_mutation` | Catches the "timeline rows + no user mutation" case. |
| S8 | `commit/cache.rs` visibility-failure path | Skip `record_unresolved` on visibility failure | `cache_commit_visible_publication_failure_reports_applied_not_visible_and_releases_guard` | Pins the AppliedButNotVisible gate recording. |
| S9 | `lifecycle/durable/bootstrap.rs` / `lifecycle/cache.rs` branch-lifecycle wrappers | Skip `try_begin_quiesce` on clear/delete/fork wrappers | `durable_{clear,delete,fork_*}_requires_quiesce_and_rejects_when_branch_guard_active`, `cache_{clear,delete,fork_*}_requires_quiesce_and_rejects_when_branch_guard_active` | Phase 4 quiesce wiring; 10 wrappers + 11 tests. |
| S10 | `lifecycle/durable/close.rs` | Drop the unresolved-durable-gate clean-state check | `durable_close_does_not_report_complete_with_unresolved_durable_gate` | Phase 3 audit gap already shipped. |
| S11 | `lifecycle/branch_lifecycle.rs::set_parent_for_recovery` | Remove the `RecoveryExclusivityToken` parameter or call from outside bootstrap | `recovery_exclusivity_token_is_minted_only_in_bootstrap` | Phase 5 compile-time enforcement + source guard. |
| S12 | `commit/branch_registry.rs::mark_deleting` | Call from outside `delete_branch` | `mark_deleting_is_only_called_from_delete_branch` | Phase 3 source guard with helper-validation sub-tests. |
| S13 | `branch/read.rs::for_inherited_layer` | Remove the `fork_version` cap | `forked_branch_isolated_from_parent_post_fork_commits` | Phase 6 fork-inheritance contract. |
| S14 | `commit/cache.rs` apply-success path | Advance `visible_version` even when publication fails | `cache_applied_not_visible_row_is_visible_to_same_branch_read_your_writes` | Phase 6 RYW pin; visible-version must not advance. |
| S15 | `commit/durable_gate.rs::replace_exact` | Skip the "different existing fact" rejection | `unresolved_durable_gate_replaces_only_exact_existing_fact` | Audit-flagged "replay gate replace_exact" path covered by existing test. |

#### Command Matrix

| Command | What it verifies | Phase |
|---|---|---|
| `cargo fmt --package strata-storage-next --check` | Formatting consistency | All |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | Lint cleanliness | All |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | Milestone-label absence, source-vocabulary hygiene, recovery-exclusivity-token scope, pre-L9 crate-private surface | 2, 5 |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | Commit-runtime isolation, `mark_deleting` scope | 2, 3 |
| `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` | Branch-LSM isolation | 2 |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | Closeout-inventory, Q-Z assurance | 1, 7 |
| `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_closeout` | Fuzz inventory + generated counter coverage | 3 |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_branch_lifecycle` | Quiesce wiring across runtimes | 4 |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_recovery` | Replay + generation guards + recovery boundary | 5 |
| `cargo test -p strata-storage-next --features testkit --locked --lib` | Full lib suite including Phase 6 pinning tests | All |

#### Disposition Summary

| Audit Item | Disposition |
|---|---|
| 15 plan-listed fault windows | 1 covered by existing `fault_replay_visible_publication_failure_records_durable_not_visible`; 14 annotated in test plan as covered by existing `tests/lifecycle_faults.rs` (19 tests) and `commit/tests/*` rejection tests. |
| 4 audit-flagged edge-case fault tests | `_replay_gate_replace_exact` covered by `unresolved_durable_gate_replaces_only_exact_existing_fact` (`commit/tests/durable_gate.rs:408`). `_conflict_validation_panic_safe` covered structurally by Rust RAII guards (Drop on unwind). `_after_allocation_partial_rollback` covered by `cache_commit_apply_failure_releases_guard_*` and `durable_apply_failure_after_wal_success_*`. `_replay_partial_wal_record` covered by `fault_partial_wal_tail_strict_fails_before_repair` + `fault_partial_wal_tail_lossy_repairs_and_degrades_health`. |
| 5 audit-recommended fuzz targets | Not adopted. Existing 4 targets (`commit_runtime_{batch,conflict,durable,timeline}`) cover the audit's hardening intent. The 2 net-new (`commit_hardening_{quiesce,checkpoint_policy}`) are structurally covered by Phase 4 wrapper tests (10+) and Phase 4 WAL-growth tests (16). |
| 8 Q-Z closeout tests | 4 shipped: `lifecycle_hardening_closeout_lists_q_to_z_plans`, `_fuzz_targets_are_distinct`, `_sensitivity_ledger_has_mutation_rows`, `_pre_l9_public_surface_is_crate_private`. 4 deferred as redundant with existing closeout tests. |

#### L8Z Closeout Summary

L8Z was carved into 7 phases via the audit-and-followup doc:

- **Phase 1** — Plan corrections (11 edits across impl plan + test plan)
- **Phase 2** — Milestone-label sweep + source-guard widening (13 prose rephrases + 6 test renames + 3 new source-guard tests + shared `source_guard_helpers` module)
- **Phase 3** — Durable gate consolidation + `mark_deleting` source guard (1 new source-guard test with helper sub-tests; 4 audit gaps shipped already)
- **Phase 4** — Quiesce wire-up for branch-lifecycle (10 wrapper edits + 11 tests + `guard_set()` accessor)
- **Phase 5** — Branch-generation guard plumbing (`RecoveryExclusivityToken` + source guard + `lookup_descriptor` accessor; replay-safety fix deferred to a future slice)
- **Phase 6** — Timeline + visibility edge-case pinning (3 fork-inheritance tests + 1 cache RYW test + `for_inherited_layer` docstring + cache.rs source comment)
- **Phase 7** — Assurance closeout (4 new Q-Z closeout tests + sensitivity ledger + command matrix + test plan annotations)

Remaining deferred items (carry forward to future slices):

- **Replay-safety fix**: stale-generation WAL records below the live `created_at` get silently applied to live state after delete+recreate. Three options remain (WAL format change, catalog manifest format change, recovery-time filter with refactored `created_at` semantics); deferred to a dedicated slice.
- **`commit_hardening_*` fuzz target aliases**: not adopted; documented rationale in test plan §"Fuzz Targets". Future slice may add explicit aliases if a reviewer wants the audit-faithful naming.

#### Verification

| Check | Result |
|---|---|
| `cargo fmt --package strata-storage-next --check` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::durable_gate` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --lib commit::tests::replay` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_closeout` | PASS, 15 tests (11 existing + 4 new) |
| `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_closeout` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_faults` | PASS |
| `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_faults` | PASS |
| `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` | PASS |
| `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` | PASS |
| `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` | PASS |
| `git diff --check` | PASS |
