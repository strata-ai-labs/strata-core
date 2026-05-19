# M4-L6 Porting Log

Status: active during M4-L6

## Purpose

This document records how branch-isolated LSM behavior moves from the current
`crates/storage` implementation into `crates/storage-next` during M4-L6.

The M4-L6 implementation plan owns order and scope. This log owns the porting
audit trail: what was read, what was preserved, what changed, what was
deferred, and what old code became eligible for retirement.

## Rules

1. Add or update a slice entry before changing storage-next branch code.
2. Prefer porting, splitting, and tightening existing storage behavior over
   fresh implementation.
3. Fresh implementation is allowed only when the entry records why existing
   behavior is obsolete, out of scope, or inconsistent with V1.
4. Do not delete old storage code until replacement tests exist and workspace
   references are gone.
5. If old code cannot be deleted because current crates still depend on it,
   record it as legacy-retained instead of adding compatibility glue to
   storage-next.
6. Treat old tests as evidence, not authority. Preserve cases that still match
   V1 semantics; reject or rewrite cases that freeze obsolete behavior.
7. Keep L6 storage-owned. Do not port `VersionedValue`, product `Value`, old
   `Key`, `Namespace`, `TypeTag`, or branch workflow DTOs into storage-next
   branch runtime code.

## Entry Template

```md
## <Slice>: <Title>

### Current Files Read

- `crates/storage/src/...`

### Behavior Preserved

- ...

### Intentional V1 Changes

- ...

### Deferred

- ...

### Tests Ported Or Added

- ...

### Sensitivity Probes

- ...

### Retirement

- Deleted:
- Legacy-retained:
- Follow-up:
```

## Baseline Source Map

| Target area | Current source material | Initial disposition |
|---|---|---|
| Branch state | `crates/storage/src/segmented/mod.rs` | Port branch-local active/frozen/immutable/inherited state after splitting out L5/L7/L8 behavior. |
| Row ordering | `crates/storage/src/key_encoding.rs` | Preserve physical-key plus descending-version row-chain semantics using storage-next row/key types. |
| Active/frozen rows | `crates/storage/src/memtable.rs` | Rebuild on L5 mutable/frozen tables. |
| MVCC selection | `crates/storage/src/merge_iter.rs` | Port visible-row grouping into L6 over L5 cursors. |
| Inherited key rewriting | `crates/storage/src/seekable.rs`, `crates/storage/src/segmented/mod.rs` | Port source-to-child branch-id rewrite and fork-version gates. |
| Immutable levels | `crates/storage/src/segment.rs`, `crates/storage/src/segmented/mod.rs` | Rebuild over L5 immutable table readers and table facts. |
| Shared refs | `crates/storage/src/segmented/ref_registry.rs` | Rebuild as runtime acceleration over durable reachability facts. |
| Branch manifests | `crates/storage/src/manifest.rs` | Use as evidence for reachability payloads; L4 owns durable publication. |
| Branch compaction | `crates/storage/src/segmented/compaction.rs` | Keep branch candidate/install/safety facts in L6; scheduling moves to L8, table mechanics stay in L5. |
| Snapshot row install | `crates/storage/src/durability/decoded_snapshot_install.rs` | Port generic row install preflight and branch-state install; recovery orchestration stays L8. |

## Slice Entries

## L6A: Branch Runtime Scaffold

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/table/mod.rs`
- `crates/storage-next/src/testkit/table_runtime.rs`
- `crates/storage-next/tests/table_runtime_source_guard.rs`
- `crates/storage-next/tests/table_runtime_properties.rs`

### Behavior Preserved

- Preserved the storage-owned vocabulary for branch state, branch read bounds,
  selected row sources, inherited layers, table descriptors, reachability facts,
  and branch runtime stats.
- Preserved the core row-chain premise from the current storage code:
  branch-aware physical key plus descending commit version produces retained
  row history. L6A records this through type shells only; concrete key helpers
  land in L6B.
- Preserved the distinction between branch-local rows, branch-owned immutable
  tables, and inherited layers as separate source facts.
- Preserved source-chain behavior for lower table errors and future publish
  errors instead of collapsing them into strings.

### Intentional V1 Changes

- Did not port `VersionedValue`, product `Value`, old `Key`, `Namespace`,
  `TypeTag`, graph/vector/search DTOs, or product branch workflow types.
- Kept all branch runtime production types `pub(crate)` and the crate root
  branch module private.
- Kept L6 independent of backend IO, filesystem paths, WAL, checkpoint,
  lifecycle orchestration, and engine crates.
- Rebuilt the generated scaffold route under storage-next `testkit` instead of
  reusing old mixed-layer tests.

### Deferred

- L6B owns branch row identity, branch-local physical-key validation, branch-id
  rewriting, and effective read-bound comparisons.
- L6C owns branch-local mutable/frozen state and committed-row append.
- L6D owns pinned read views and own-branch latest/getv/history/prefix/range
  reads.
- L6E owns branch-owned immutable table levels.
- L6F owns fork metadata, inherited-layer read behavior, and source-to-child
  key rewriting.
- L6G owns timestamp-bounded reads and TTL visibility.
- L6H owns materialization mechanics.
- L6I owns reachability/shared-table registry behavior.
- L6J owns branch compaction state transitions.
- L6K owns snapshot row install.
- L6L owns full L6 conformance closeout.

### Tests Ported Or Added

- Added `crates/storage-next/src/branch/tests.rs` for config, read-bound,
  fact, descriptor, row-result, stats, full error-vocabulary, and
  error/source-chain scaffold tests.
- Added `crates/storage-next/tests/branch_lsm_source_guard.rs` with source
  guards for upper-layer imports, product DTO vocabulary, backend/IO/lifecycle
  APIs, public surface leakage, and L6A-scaffold-only premature branch behavior
  entrypoints. The guard includes self-tests for forbidden and allowed terms,
  including bare backend operation calls and method-call forms.
- Added `crates/storage-next/src/testkit/branch_lsm.rs` and exported
  `check_branch_lsm_scaffold_contract` plus `BranchLsmScaffoldOutcome` through
  the feature-gated testkit.
- Added `crates/storage-next/tests/branch_lsm_properties.rs` so generated
  branch scaffold scripts exercise nonzero counters across config, read bounds,
  facts, descriptors, errors, and stats.

### Sensitivity Probes

- `branch_lsm_source_guard_catches_required_forbidden_terms` verifies the guard
  catches upper-layer imports, filesystem/path vocabulary, direct backend API
  calls in both bare-call and method-call forms, backend/service imports,
  public-surface leakage, and product DTO drift while allowing storage-owned
  branch/table/row vocabulary.
- `branch_lsm_source_has_no_premature_behavior_entrypoints` verifies the L6A
  scaffold files do not introduce read, fork, materialization, snapshot-install,
  append, or compaction behavior before the owning L6 slices land. Later
  behavior-owning slices must narrow or retire this guard as they add those
  concrete entrypoints.
- `branch_runtime_config_rejects_unusable_zero_limits` verifies zero scaffold
  limits fail as typed `InvalidConfig` errors.
- `branch_state_facts_accept_empty_shape_and_reject_impossible_shapes` verifies
  impossible timestamp bounds, empty-branch max-version facts, and empty-branch
  timestamp ranges fail as typed `InvalidBranchState` errors.
- `branch_descriptors_preserve_storage_owned_facts` verifies descriptor identity
  mismatches fail with typed branch-state errors instead of relying on
  debug-only assertions, and that descriptor debug/equality behavior remains
  fact-based.
- `branch_lsm_property_harness_runs_scaffold_contract` verifies generated
  scripts exercise every scaffold category with nonzero counters.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
- `cargo fmt --package strata-storage-next --check`
- `git diff --check`

### Retirement

- Deleted: none.
- Legacy-retained: `crates/storage/src/segmented/*`, `key_encoding.rs`,
  `memtable.rs`, `merge_iter.rs`, `seekable.rs`, and related current-storage
  tests remain in place because current crates still depend on them.
- Follow-up: L6B-L6L will retire old behavior only after replacement mechanics
  and conformance tests exist in storage-next.

## L6B: Branch Row Identity And Read Bounds

### Current Files Read

- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/table/key.rs`
- `crates/storage-next/src/branch/{mod.rs,read.rs,error.rs,facts.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6b-branch-row-identity-read-bounds-test-plan.md`

### Behavior Preserved

- Preserved the rule that own-branch rows must carry the expected branch id in
  their physical key before branch state may accept them.
- Preserved inherited-row branch-id rewrite semantics from the old
  `RewritingIterator`/`RewritingSeekableIter`: source rows are projected into
  the target branch namespace without changing row version, timestamp, expiry,
  tombstone, user-key, storage-space, or value facts.
- Preserved the old inherited fork-version gate as a version cap on inherited
  reads.
- Preserved inclusive version and timestamp read-bound comparisons:
  `row.version <= cap` and `row.timestamp <= cap`.
- Preserved tombstone and expired-looking rows as storage facts during
  candidate classification. Final live-value policy remains later L6 behavior.

### Intentional V1 Changes

- Rebuilt branch-id rewrite through storage-next `PhysicalKey` and
  `StorageRow` constructors instead of mutating encoded key bytes directly.
- Represented inherited timestamp reads as a combined effective bound with both
  `max_commit_version = fork_version` and `max_commit_timestamp = requested`.
  This makes the fork gate explicit instead of coupling it to iterator control
  flow.
- Kept L6B as a pure helper layer: no branch state, table reads, backend IO,
  lifecycle orchestration, commit runtime, or product DTO conversion.

### Deferred

- L6C owns branch-local mutable/frozen state and committed-row append.
- L6D owns final own-branch latest/getv/history/prefix/range selection.
- L6F owns inherited-layer iteration, seek-bound rewrite, and child-local
  shadowing.
- L6G owns timestamp-read live-value policy and TTL visibility.
- L6H owns materialization using the L6B rewrite helpers.
- L6J owns compaction policy integration using L6B candidate facts.

### Tests Ported Or Added

- Added `crates/storage-next/src/branch/identity.rs` for branch-local
  physical-key validation, row identity construction, and lossless branch-id
  rewrite helpers.
- Extended `crates/storage-next/src/branch/read.rs` with
  `BranchEffectiveReadBound`, `BranchRowBoundMatch`, and
  `BranchRowCandidateFacts`.
- Extended `crates/storage-next/src/branch/tests.rs` with direct tests for
  matching/wrong-branch rows, put/tombstone rewrite preservation, inclusive
  own and inherited bounds, combined timestamp plus fork caps, and candidate
  facts that preserve tombstone/expiry facts.
- Added direct edge tests for the complete L6B test-plan envelope:
  `branch_physical_key_validation_accepts_opaque_edge_key_shapes`,
  `branch_row_validation_accepts_put_tombstone_and_edge_rows_without_policy`,
  `branch_rewrite_preserves_empty_put_values_and_storage_owned_keys`,
  `branch_own_bounds_cover_zero_epoch_and_below_equal_above_edges`,
  `branch_inherited_bounds_cover_fork_edges_and_combined_timestamp_match`,
  and `branch_candidate_bound_match_records_each_axis_independently`.
- Added direct row-chain and encoded-grouping coverage:
  `branch_effective_bounds_filter_row_chains_without_collapsing_versions`
  verifies version/timestamp intersection without selecting one visible row,
  and
  `branch_rewrite_groups_inherited_rows_with_child_local_encoded_keys`
  verifies branch rewrite places inherited rows in the child-local encoded key
  group.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` so generated
  scripts exercise L6B row identity, mismatch rejection, rewrites, own bounds,
  inherited bounds, candidate facts, storage-owned/empty-key edge rows,
  encoded-key grouping, row-chain filtering, and fork-edge caps.
- Extended `crates/storage-next/tests/branch_lsm_properties.rs` to require
  nonzero generated counters for the L6B categories.

### Sensitivity Probes

- `branch_row_identity_accepts_matching_rows_and_rejects_mismatches` verifies
  wrong-branch rows fail with typed branch-row errors before state mutation.
- `branch_physical_key_validation_accepts_opaque_edge_key_shapes` verifies
  storage-owned and engine-owned storage-space ids, empty and high-bit user
  keys, opaque branch-id bytes, same-branch physical-key rewrite, and
  source-to-target-to-source key rewrite.
- `branch_row_validation_accepts_put_tombstone_and_edge_rows_without_policy`
  verifies put/tombstone row validation preserves zero/MAX version and
  timestamp facts without applying TTL or tombstone visibility policy.
- `branch_rewrite_preserves_put_and_tombstone_row_facts` verifies branch-id
  rewrite preserves put and tombstone row facts and rejects an unexpected
  source branch.
- `branch_rewrite_preserves_empty_put_values_and_storage_owned_keys` verifies
  empty put values and storage-owned keys survive row rewrite unchanged except
  for the branch id.
- `branch_effective_read_bounds_apply_inclusive_own_and_inherited_caps`
  verifies inclusive version/timestamp caps and inherited fork-version caps,
  including combined inherited timestamp bounds.
- `branch_own_bounds_cover_zero_epoch_and_below_equal_above_edges` verifies
  `CommitVersion::ZERO`, `Timestamp::EPOCH`, below/equal/above comparisons, and
  latest own-branch bounds.
- `branch_inherited_bounds_cover_fork_edges_and_combined_timestamp_match`
  verifies inherited latest, `AtVersion` below/equal/above fork, and combined
  timestamp plus fork-version matching.
- `branch_candidate_facts_preserve_tombstone_and_expiry_without_visibility_policy`
  verifies L6B candidate classification does not hide tombstones or
  expired-looking rows.
- `branch_candidate_bound_match_records_each_axis_independently` verifies
  version and timestamp miss facts are recorded independently before final
  bound-match conjunction.
- `branch_rewrite_groups_inherited_rows_with_child_local_encoded_keys`
  verifies inherited row rewrite preserves the logical physical key after
  projection into the child branch and sorts newest-first within that group.
- `branch_effective_bounds_filter_row_chains_without_collapsing_versions`
  verifies row-chain filtering remains an inclusive fact pass and does not
  collapse tombstones or expired-looking rows into a final visible result.
- `branch_lsm_property_harness_runs_scaffold_contract` now verifies generated
  scripts exercise the L6B helper categories, row-chain cases, encoded-grouping
  cases, storage-owned edge rows, and fork-edge caps with nonzero counters.
- Temporary mutation-probe outcomes are pending. The direct and generated
  regression tests above cover the probe categories, but L6B should not be
  marked final-closeout until the temporary mutations listed in the L6B test
  plan are run and recorded.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
- `cargo fmt --package strata-storage-next --check`
- `git diff --check`

### Retirement

- Deleted: none.
- Legacy-retained: old `RewritingIterator`, `RewritingSeekableIter`, and
  encoded-key branch rewrite logic remain because current storage still depends
  on them.
- Follow-up: L6F/L6H can retire more inherited-layer rewrite behavior after
  iterator/materialization replacements land in storage-next.

## L6C: Branch-Local Mutable And Frozen State

### Current Files Read

- `crates/storage/src/memtable.rs`
- `crates/storage/src/segmented/mod.rs`
- `crates/storage-next/src/table/mutable.rs`
- `crates/storage-next/src/table/key.rs`
- `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6c-branch-local-mutable-frozen-state-test-plan.md`

### Behavior Preserved

- Preserved the old branch-local state split between one active in-memory table
  and a newest-first list of frozen in-memory tables.
- Preserved committed-row installation as an internal-key ordered table
  mutation, now delegated to L5 `MutableTable`.
- Preserved exact duplicate internal-key rejection and extended the preflight
  across both active and frozen branch-local tables before mutation.
- Preserved mechanical max commit version and timestamp min/max accounting
  across active and frozen rows.
- Preserved put and tombstone rows as storage facts without applying final
  live-value, TTL, or deletion visibility policy.
- Preserved the frozen-limit safety behavior: when the configured frozen-table
  cap is reached, active rows stay active and no rows are dropped.

### Intentional V1 Changes

- Rebuilt the state on storage-next `StorageRow`, `MutableTable`, and
  `FrozenTable` instead of old memtable entries, skiplist internals, bloom
  filters, wall-clock write paths, or product DTOs.
- Used L6B `require_row_branch` for branch-id validation before every append.
- Modeled empty rotation and frozen-limit rotation as explicit
  `BranchRotationOutcome::Skipped` cases rather than errors.
- Kept L6C entirely in-memory and branch-local: no WAL append, backend IO,
  object layout, immutable table object install, manifest publication, or
  lifecycle scheduling.

### Deferred

- L6D owns pinned own-branch latest/getv/history/prefix/range reads over this
  active/frozen state.
- L6E owns branch-owned immutable table levels and object-backed table install.
- L6F owns inherited-layer iteration and child-local rewrite in read views.
- L6G owns timestamp/as-of live-value policy and TTL visibility.
- L6J owns branch compaction state transitions and immutable output install.
- L6K owns snapshot row install.
- L8 owns WAL-before-visible discipline, flush scheduling, and durable
  lifecycle orchestration.

### Tests Ported Or Added

- Extended `crates/storage-next/src/branch/state.rs` with `BranchLocalState`,
  `BranchAppendOutcome`, `BranchRotationOutcome`, and
  `BranchRotationSkipReason`.
- Added direct branch tests for empty construction, successful put/tombstone
  appends, wrong-branch rejection without mutation, active and frozen duplicate
  rejection without mutation, same physical key at different versions,
  different keys at the same version, active rotation, newest-first frozen
  ordering, frozen-limit skip, branch-local facts, and zero/MAX
  version/timestamp edge facts.
- Added direct L6C append coverage for opaque branch ids, storage-owned keys,
  empty user keys, NUL-containing user keys, high-bit user-key bytes, and
  distinct space names with shared prefixes.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` so generated scripts
  exercise state construction, put/tombstone append, wrong-branch rejection,
  active/frozen duplicate rejection, valid row-chain appends, active rotation,
  empty rotation, frozen-limit skip, append-after-frozen-limit-skip, zero/MAX
  fact edges, and active/frozen/mixed fact accounting.
- Extended `crates/storage-next/tests/branch_lsm_properties.rs` to require
  nonzero counters for every L6C generated category.
- Narrowed `crates/storage-next/tests/branch_lsm_source_guard.rs` so L6C-owned
  append and rotation entrypoints are allowed while read, fork, materialize,
  immutable install, compaction, snapshot install, backend, lifecycle, product
  DTO, and public-surface drift remain forbidden.

### Sensitivity Probes

- `branch_local_state_rejects_wrong_branch_rows_without_mutation` covers
  accepting a wrong-branch row and updating facts before validation.
- `branch_local_state_rejects_active_and_frozen_duplicates_without_mutation`
  covers allowing duplicate active or frozen internal keys and confirms facts
  stay unchanged on failure.
- `branch_local_state_appends_puts_tombstones_and_preserves_row_facts` covers
  rejecting same physical key at a different commit version, rejecting
  different physical keys at the same commit version, dropping tombstones,
  dropping empty put values, and timestamp/max-commit fact drift.
- `branch_local_state_tracks_zero_max_version_and_timestamp_edges` covers
  `CommitVersion::ZERO`, `CommitVersion::MAX`, `Timestamp::EPOCH`, and
  `Timestamp::MAX` in active and frozen facts.
- `branch_local_state_rotation_preserves_rows_and_newest_first_order` covers
  reset-on-rotation bugs, oldest-first frozen insertion, and empty-rotation
  frozen-table creation.
- `branch_local_state_respects_frozen_limit_without_dropping_active_rows`
  covers dropping active rows or mutating frozen rows on frozen-limit skip.
- `branch_lsm_property_harness_runs_scaffold_contract` covers the same
  categories through generated scripts.
- `branch_lsm_source_guard_catches_required_forbidden_terms` and
  `branch_lsm_source_guard_catches_backend_operation_call_forms` cover product
  DTO and backend-call mutation probes.
- No separate L6C probe task remains open; the categories are represented by
  permanent regression tests.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`

### Retirement

- Deleted: none.
- Legacy-retained: old memtable and segmented branch-state code remain because
  current storage still depends on them.
- Follow-up: L6D/L6E/L6J/L8 will retire more old branch-state behavior after
  read views, immutable install, compaction, and lifecycle replacement slices
  land in storage-next.
