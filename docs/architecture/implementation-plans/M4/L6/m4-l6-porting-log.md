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
  `BranchEffectiveReadBound` and `BranchRowCandidateFacts`.
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
  and `branch_candidate_bound_facts_record_each_axis_independently`.
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
- `branch_candidate_bound_facts_record_each_axis_independently` verifies
  version and timestamp miss facts are recorded independently before final
  bound conjunction.
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

## L6D: Pinned Own-Branch Read Views

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/memtable.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage-next/src/table/{cursor.rs,key.rs,mutable.rs}`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6d-pinned-own-branch-read-views-test-plan.md`

### Behavior Preserved

- Preserved the old `BranchSnapshot` pinning rule: a read view captures the
  active/frozen source set and later branch-local appends or rotations do not
  change what that view sees.
- Preserved own-branch row-chain selection by physical key and descending
  commit version, including `latest` and version-bounded reads.
- Preserved tombstone shadowing for visible reads: if the selected in-bound row
  is a tombstone, the read returns `None` and does not fall through to an older
  put.
- Preserved retained history reads newest first, including tombstones by
  default, with exclusive `before_version` and post-filter `limit` handling.
- Preserved prefix and range scan behavior as one selected visible row per
  physical key, ordered by encoded physical key and constrained to the
  requested branch, space, and storage-space id.
- Preserved source facts for selected rows as `Active` or `Frozen { index }`.

### Intentional V1 Changes

- Rebuilt the old memtable/merge-iterator read path over storage-next
  `MutableTable`, `FrozenTable`, `StorageRow`, and L6 row-result shells.
- Used cloned L5 table snapshots for the first pinned view implementation. This
  is simpler than the old `Arc` snapshot shape and is acceptable until immutable
  table integration or retention pressure requires reference-counted views.
- Rejected timestamp/as-of read bounds with typed `InvalidReadBound` errors in
  L6D. L6G owns timestamp-bounded visibility and TTL policy.
- Kept reads storage-owned: no `VersionedValue`, product `Value`, old `Key`,
  `Namespace`, `TypeTag`, backend IO, layout constructors, lifecycle APIs, or
  engine DTOs were introduced.

### Deferred

- L6E owns branch-owned immutable table levels and object-backed table reads.
- L6F owns inherited-layer reads, child-local/inherited shadowing, and
  source-to-child branch-id rewrite in read views.
- L6G owns timestamp/as-of reads and TTL-at-read-time policy.
- L6H owns materialization mechanics.
- L6J owns branch compaction state transitions.
- L6K owns snapshot row install.
- L8 owns WAL-before-visible discipline, flush scheduling, and durable
  lifecycle orchestration.

### Tests Ported Or Added

- Extended `crates/storage-next/src/branch/read.rs` with `BranchReadView`,
  `BranchScanBounds`, `BranchUserKeyBound`, and `BranchHistoryOptions`.
- Extended `crates/storage-next/src/branch/state.rs` with
  `BranchLocalState::capture_read_view`.
- Added direct branch tests for pinned append/rotation isolation, latest and
  version-bounded reads across active/frozen sources, tombstone shadowing,
  history including tombstones, `before_version`, zero and one-row limits,
  empty and single-row views, frozen-limit skip pinning, zero/MAX commit
  version bounds, multiple frozen-table source attribution, prefix scans,
  closed/open/manual/unbounded range scans, embedded-zero user-key prefixes,
  degenerate ranges, wrong-branch point and scan rejection, timestamp-bound
  deferral, invalid range bounds, invalid direct scan spaces, and read-view
  constructor rejection for stale facts, active/frozen wrong-branch source rows,
  mismatched frozen facts, and unsupported immutable/inherited source facts.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` so generated scripts
  exercise read-view capture, pinned append/rotation isolation, latest reads,
  version-bounded reads, tombstone shadowing, history reads, history tombstone
  preservation, history limits, prefix scans, range scans, scan tombstone
  suppression, active/frozen merge selection, wrong-branch read rejection, and
  timestamp-bound deferral.
- Extended `crates/storage-next/tests/branch_lsm_properties.rs` to require
  nonzero generated counters for every L6D read-view category.
- Narrowed `crates/storage-next/tests/branch_lsm_source_guard.rs` so L6D-owned
  read-view methods are allowed while product DTO, backend, lifecycle,
  wall-clock, fork, install, materialize, snapshot, compaction, and public
  surface drift remain forbidden.

### Sensitivity Probes

- `branch_read_view_is_pinned_across_append_and_rotation` catches views that
  alias mutable active/frozen state rather than pinning the captured state.
- `branch_read_view_latest_and_version_reads_follow_row_chain_not_source_order`
  catches source-priority bugs where active rows incorrectly beat newer frozen
  rows, plus tombstone fallthrough bugs.
- `branch_read_view_empty_and_single_row_cases_are_stable` catches empty view
  handling, single-row edge behavior, and premature expiry filtering.
- `branch_read_view_frozen_limit_skip_does_not_mutate_captured_view` catches
  frozen-limit skip mutations that would leak into an already captured view.
- `branch_read_view_version_bounds_respect_tombstone_edges_and_extremes` catches
  inclusive tombstone-bound mistakes and zero/MAX commit-version boundary
  regressions.
- `branch_read_view_multiple_frozen_tables_preserve_source_facts` catches
  newest-row selection across multiple frozen tables and incorrect active vs
  frozen source attribution.
- `branch_read_view_history_preserves_tombstones_limits_and_before_version`
  catches dropped tombstones, inclusive `before_version` mistakes, limit-zero
  mistakes, empty-value loss, and expiry-fact filtering before L6G.
- `branch_read_view_prefix_and_range_scans_group_by_physical_key` catches scans
  that return multiple versions for one physical key, cross space or
  storage-space-id boundaries, ignore open/closed bounds, skip high-bit user
  keys, or fail to suppress a tombstoned physical key.
- `branch_read_view_scans_cover_empty_prefix_zero_bytes_and_degenerate_ranges`
  catches empty-prefix scans, embedded-zero prefix handling, same-user-key
  storage-space boundary leaks, and open/closed degenerate range mistakes.
- `branch_read_view_constructor_rejects_stale_facts_and_wrong_branch_sources`
  catches stale captured facts, wrong-branch source rows, unsupported
  immutable/inherited fact counts, and payload leaks during constructor
  rejection.
- `branch_read_view_constructor_rejects_frozen_source_and_fact_mismatches`
  catches frozen-table count drift, captured timestamp drift, wrong-branch
  frozen rows, and payload leaks during frozen-source constructor rejection.
- `branch_read_view_rejects_wrong_branch_and_timestamp_bounds_without_payload`
  catches wrong-branch point/scan acceptance, timestamp/as-of implementation
  before L6G, invalid direct scan spaces, and error payload leaks.
- `branch_lsm_property_harness_runs_scaffold_contract` covers the same
  categories through generated scripts.

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
- Legacy-retained: old `BranchSnapshot`, memtable versioned reads,
  prefix/range iteration, and merge/MVCC iterator code remain because current
  storage still depends on them.
- Follow-up: L6E/L6F/L6G/L6J/L8 will retire more old read-path behavior after
  immutable levels, inherited reads, timestamp visibility, compaction, and
  lifecycle replacement slices land in storage-next.

## L6E: Branch-Owned Immutable Levels

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segment.rs`
- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage-next/src/table/{builder.rs,reader.rs,facts.rs,key.rs,mutable.rs}`
- `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6e-branch-owned-immutable-levels-test-plan.md`

### Behavior Preserved

- Preserved the old segmented storage shape where a branch owns active,
  frozen, and immutable level sources.
- Preserved L0 as newest-first and overlap-tolerant.
- Preserved L1+ as sorted and non-overlapping by immutable table key range.
- Preserved frozen-table flush replacement as a visible-read-preserving state
  transition: the replacement table must contain the same `StorageRow`s as the
  named frozen table, and the frozen table is removed only after validation.
- Preserved row-chain selection by commit version across active, frozen, and
  branch-owned immutable sources.
- Preserved pinned read-view isolation after immutable installs and frozen
  replacement by cloning the branch-owned level layout into the read view.

### Intentional V1 Changes

- Rebuilt immutable branch levels over L5 `ImmutableTableReader`,
  `TableRuntimeFacts`, `BranchTableDescriptor`, and storage-next `StorageRow`
  rather than old `SegmentVersion` structures.
- Added `BranchOwnedTable` as the branch-owned L5 reader wrapper. It validates
  descriptor facts and branch id ownership before a table can enter branch
  state.
- Added in-memory install helpers on `BranchLocalState`:
  `install_l0_table`, `install_owned_table_at_level`, and
  `replace_frozen_with_l0_table`.
- Kept all durable publication, table-object loading, manifest update, flush
  scheduling, and WAL-before-visible ordering out of L6E.

### Deferred

- L6F owns inherited layers and child-local/inherited read merging.
- L6G owns timestamp/as-of reads and TTL-at-read-time policy.
- L6H owns materialization mechanics.
- L6I/L8 own durable branch manifest/reachability publication and recovery.
- L6J owns compaction candidate selection and replacement of old immutable
  table sets.
- L6K owns snapshot row install.

### Tests Ported Or Added

- Extended `crates/storage-next/src/branch/read.rs` with `BranchOwnedTable`
  and immutable-source candidate collection for point, history, prefix, and
  range reads.
- Extended `crates/storage-next/src/branch/state.rs` with branch-owned
  immutable level storage, L0 install, nonzero-level install, frozen-to-L0
  replacement, duplicate-key validation, and branch facts that include
  immutable rows.
- Added direct branch tests for descriptor/fact mismatch rejection,
  wrong-branch immutable row rejection without payload leakage, owned-table
  branch-id retention, empty immutable input rejection before branch install,
  cross-branch install rejection without mutation, L0 install and source
  attribution, frozen replacement with pinned-view
  isolation, L1 sorted non-overlap validation, failed-install non-mutation,
  install-level mismatch, configured-level overflow rejection,
  frozen-replacement row mismatch and out-of-range rejection, immutable
  prefix/range/tombstone scans, pinned views captured before L0 install, and
  row-chain reads across active/frozen/owned immutable sources.
- Added direct branch tests for overlapping L0 install order versus commit
  version selection, named frozen replacement with multiple frozen tables and
  preexisting L0 tables, active-vs-L0 and frozen-vs-L0 point-read precedence,
  L1 point reads, immutable version tombstone edges, zero/max commit-version
  reads, immutable history tombstone filtering and limits across levels, and
  immutable prefix/range scans over active, frozen, L0, and L1 sources.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` and
  `crates/storage-next/tests/branch_lsm_properties.rs` with generated L6E
  categories for immutable descriptor construction, L0/L1 install, invalid
  install rejection, L1 overlap rejection, frozen replacement, pinned install
  isolation, latest/version/history reads, prefix/range scans, tombstone
  shadowing, active/frozen/immutable merge reads, and immutable source
  attribution.
- Narrowed `crates/storage-next/tests/branch_lsm_source_guard.rs` so L6E-owned
  immutable install entrypoints are recognized while backend, lifecycle,
  product DTO, fork, materialization, snapshot, compaction, and public-surface
  drift remain forbidden.

### Sensitivity Probes

- `branch_owned_table_constructor_rejects_descriptor_and_branch_mismatches`
  catches descriptor/fact mismatch, wrong-branch table acceptance, and payload
  leakage in immutable-table validation errors.
- `branch_local_state_installs_l0_table_and_reads_owned_sources` catches L0
  install fact drift and missing owned-table read candidate collection.
- `branch_local_state_replaces_frozen_with_l0_without_mutating_pinned_views`
  catches replacement order bugs, loss of frozen rows in pinned views, and
  source-attribution drift after replacement.
- `branch_frozen_replacement_rejects_mismatches_without_mutation` catches
  frozen replacement that drops the frozen table before validating equivalent
  rows or accepts an out-of-range frozen index.
- `branch_owned_nonzero_levels_are_sorted_and_reject_overlaps_without_mutation`
  catches unsorted L1+ insertion, overlap acceptance, level mismatch, and
  failed-install mutation.
- `branch_read_view_merges_owned_tables_with_active_and_frozen_by_commit_version`
  catches source-priority bugs where owned immutable rows are ignored or source
  order beats commit-version visibility.
- `branch_local_state_rejects_owned_table_for_other_branch_without_mutation`
  catches branch-owned table wrappers being accepted into the wrong branch
  state after construction.
- `branch_read_view_scans_owned_immutable_tables_and_pins_before_l0_install`
  catches missing immutable prefix/range scan participation, immutable
  tombstone fall-through, and read-view mutation after later L0 install.
- `branch_owned_l0_tables_accept_overlaps_and_select_by_version_not_index`
  catches L0 overlap rejection and source-order bugs where table index hides a
  newer commit version.
- `branch_frozen_replacement_targets_named_frozen_table_and_keeps_l0_front`
  catches replacing the wrong frozen table, appending replacement output behind
  older L0 tables, and mutating pinned pre-replacement views.
- `branch_immutable_point_reads_choose_newer_between_active_and_l0` and
  `branch_immutable_point_reads_choose_newer_between_frozen_l0_and_l1` catch
  point-read precedence drift across active, frozen, L0, and L1 sources.
- `branch_immutable_version_reads_cover_tombstone_bounds` and
  `branch_immutable_version_reads_cover_zero_and_max_commit_bounds` catch
  bounded-read drift around tombstones and commit-version extremes.
- `branch_immutable_history_filters_tombstones_limits_and_cross_level_versions`
  catches dropped immutable history rows, tombstone-filter ordering bugs, and
  limit application before filtering.
- `branch_immutable_prefix_scans_merge_sources_and_respect_spaces` and
  `branch_immutable_prefix_scan_includes_l1_and_excludes_storage_space_id`
  catch scan grouping, source merge, space-boundary, storage-space-boundary,
  L1 participation, tombstone, and duplicate visible-key regressions.
- `branch_immutable_range_scans_cover_l1_edge_and_degenerate_bounds` catches
  L1 range-edge, adjacent-table over-inclusion, and degenerate-bound
  regressions.
- The generated branch-LSM property harness asserts every L6E immutable
  category counter is nonzero, preventing the immutable paths from becoming
  property-test placeholders.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`

### Retirement

- Deleted: none.
- Legacy-retained: old `SegmentVersion` level management and manifest-backed
  durable branch levels remain because current storage still depends on them.
- Follow-up: L6F/L6G/L6H/L6I/L6J/L6K/L8 will retire more old branch behavior
  after inherited reads, timestamp visibility, materialization, durable
  manifest integration, compaction, snapshot install, and lifecycle
  replacement slices land in storage-next.

## L6F: Fork And Inherited Layers

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage-next/src/branch/{config.rs,error.rs,facts.rs,identity.rs,read.rs,state.rs}`
- `crates/storage-next/src/table/{builder.rs,reader.rs,facts.rs,key.rs,mutable.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6f-fork-inherited-layers-test-plan.md`

### Behavior Preserved

- Preserved the old copy-on-write branch shape where a child branch reads
  immutable source tables through inherited layer references rather than row
  copies.
- Preserved fork-version gating: inherited rows with commit versions above the
  layer fork version are invisible to the child.
- Preserved inherited key rewrite: source branch ids are rewritten to the child
  branch before MVCC grouping and scan grouping.
- Preserved child-local precedence over inherited rows through the existing
  row-chain selector.
- Preserved nearest-ancestor-first ordering for inherited exact ties.
- Preserved pinned read-view isolation by cloning inherited layer descriptors
  and L5 reader handles into `BranchReadView`.

### Intentional V1 Changes

- Rebuilt inherited layers over L5 `ImmutableTableReader` and storage-next
  `StorageRow` instead of old `SegmentVersion`, `InternalKey`, and
  `MemtableEntry` iterators.
- Added `BranchInheritedLayer` as the L6 inherited source wrapper. It validates
  descriptor table counts, source branch ownership, inherited table level
  facts, and duplicate internal keys before a layer can enter a read view.
- Added `BranchLocalState::fork_into_empty_child` and
  `BranchLocalState::attach_inherited_layers` as in-memory storage mechanics.
  They do not publish manifests, mutate backends, or release source tables.
- Copied active/materializing inherited layers reset to `Active` in the child.
  Materialized layers are skipped because their replacement state is already
  the readable source.
- L6F does not implicitly inherit source active/frozen rows. Upper layers must
  flush/install source mutable state before invoking the fork helper when that
  behavior is required.
- L6F's shipped fork helper uses the source max applied commit version as the
  fork version. Retained historical fork-version requests remain deferred until
  a caller-owned retained-history proof API exists.

### Deferred

- L6G owns timestamp/as-of reads and TTL visibility over inherited rows.
- L6H owns materialization state transitions and read parity before/after
  materialization.
- L6I/L8 own durable reachability, shared table reference release facts,
  manifest publication, and recovery.
- L6J owns branch compaction safety across inherited/lower rows.
- L6K owns snapshot row install.
- Retained historical fork-version requests and above-source-max rejection are
  deferred until the retained-history proof API exists.
- Dedicated inheritance fuzz coverage is delivered by the L6L
  `branch_lsm_inheritance` target. The generated branch-LSM property harness
  also covers the L6F inheritance categories with dedicated counters/scripts.

### Tests Ported Or Added

- Extended `crates/storage-next/src/branch/read.rs` with inherited layer
  storage in `BranchReadView`, inherited point/history/scan candidate
  collection, per-layer fork-version filtering, and source-to-child row
  rewriting before grouping.
- Extended `crates/storage-next/src/branch/state.rs` with inherited layer
  storage, fork outcome facts, inherited attach validation, in-memory fork
  capture, and branch facts that include inherited fork-version visibility.
- Added direct branch tests for inherited descriptor/table-count validation,
  duplicate inherited internal-key rejection, wrong-source rejection without
  payload leakage, direct self-inheritance rejection, materializing/materialized/
  unavailable status behavior, inherited-layer limit enforcement, non-mutating
  attach/fork rejection, fork status reset and layer-order preservation, fork
  capture without own-row copy, source active-row non-inheritance, inherited
  latest reads, overlapping inherited L0 selection, inherited L1 point reads,
  fork-version gates, inherited history tombstone/before-version/limit filters,
  child-owned exact-duplicate shadowing, child tombstone shadowing, inherited
  scan grouping after rewrite, inherited scan named-space/storage-space/range
  edge handling, wrong-branch/timestamp read rejection without payload leakage,
  inherited history source facts, and chained nearest-ancestor tie-breaking.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` and
  `crates/storage-next/tests/branch_lsm_properties.rs` with generated L6F
  categories for fork capture, inherited layer validation, latest/versioned
  inherited reads, inherited history, prefix/range scans, source-to-child key
  rewrites, child put/tombstone shadowing, post-fork source invisibility,
  nearest-ancestor chained reads, invalid inherited-layer rejection, and pinned
  inherited view isolation.
- Updated `crates/storage-next/tests/branch_lsm_source_guard.rs` comments and
  allow-list examples so L6F fork/inheritance entrypoints are no longer
  described as premature while materialization, compaction, snapshot install,
  backend IO, lifecycle, commit-runtime, and product DTO drift remain
  forbidden.

### Sensitivity Probes

- `branch_fork_into_empty_child_captures_inherited_layers_without_copying_rows`
  catches row-copy forks, missing inherited layer facts, missing branch-id
  rewrite, and accidental source active/frozen inheritance.
- `branch_fork_preserves_layer_order_and_resets_readable_inherited_statuses`
  catches materializing-status leakage into forked children, materialized-layer
  copy-through, and source-owned/inherited layer ordering drift.
- `branch_fork_and_attach_rejections_do_not_mutate_state` catches non-empty
  inherited attach acceptance, self-fork acceptance, unavailable-layer fork
  acceptance, and partial mutation on rejected operations.
- `branch_inherited_reads_apply_fork_gate_and_child_tombstone_shadowing`
  catches omitted fork-version gates, post-fork parent visibility, and child
  tombstone fallthrough to inherited puts.
- `branch_inherited_history_filters_tombstones_limits_and_fork_gates` catches
  inherited tombstone fallthrough, tombstone-filter drift, before-version
  inclusion bugs, limit-before-filter bugs, and history exposure above the fork
  version.
- `branch_inherited_l0_overlap_and_l1_tables_participate_in_point_reads`
  catches inherited L0 overlap omission, inherited L1 omission, and source
  attribution drift in point reads.
- `branch_inherited_scans_and_history_rewrite_before_grouping` catches scan
  grouping before rewrite, inherited source-fact drift, and inherited history
  omissions.
- `branch_inherited_scans_preserve_space_boundaries_and_range_edges` catches
  inherited scan leakage across named spaces/storage-space ids and open/closed
  range edge drift after rewrite.
- `branch_inherited_rejects_wrong_branch_and_timestamp_reads_without_payload`
  catches wrong-branch validation being delayed until inherited lookup and
  accidental timestamp/as-of enablement before L6G.
- `branch_chained_fork_prefers_nearest_inherited_layer_for_exact_ties` catches
  reversed ancestry order for inherited exact ties.
- `branch_inherited_layer_constructor_rejects_count_and_source_mismatches`
  catches stale descriptor counts, wrong-source inherited tables, direct
  self-inheritance, and payload leakage during validation.
- The generated branch-LSM property harness asserts every L6F inheritance
  category counter is nonzero, preventing generated fork/inheritance coverage
  from becoming a placeholder.

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
- Legacy-retained: old fork manifest publication, ref registry barriers,
  `RewritingSeekableIter`, and segment-backed inherited layers remain because
  current storage still depends on them.
- Follow-up: L6G/L6H/L6I/L6J/L6K/L8 will retire more old inheritance behavior
  after timestamp visibility, materialization, durable reachability, branch
  compaction, snapshot install, and lifecycle replacement slices land in
  storage-next.

## L6G: Timestamp Reads And TTL Visibility

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/stored_value.rs`
- `crates/storage/src/merge_iter.rs`
- `crates/storage/src/seekable.rs`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/testkit/branch_lsm.rs`
- `crates/storage-next/tests/branch_lsm_source_guard.rs`
- `crates/storage-next/tests/branch_lsm_properties.rs`

### Behavior Preserved

- Preserved retained row-chain semantics: timestamp-bounded reads first filter
  rows by commit timestamp, then select the newest eligible row by commit
  version and existing source tie-breaks.
- Preserved separate storage rows for historical versions instead of rebuilding
  the old `VersionedValue` container.
- Preserved tombstone shadowing at timestamp bounds: a selected tombstone
  returns no visible value and does not fall through to older puts.
- Preserved inherited source-to-child key rewriting before grouping, with
  inherited timestamp reads also applying the fork-version gate.
- Preserved timestamp, expiry, tombstone, value, and source facts in retained
  history rows.

### Intentional V1 Changes

- Did not port wall-clock expiry evaluation from old storage. L6G evaluates TTL
  only against the requested timestamp read bound.
- Did not port product `Value`, old `Key`, `Namespace`, `TypeTag`, or
  `VersionedValue` vocabulary into storage-next.
- Treated `Timestamp::EPOCH` expiry on put rows as the no-expiry sentinel.
- Documented and tested exact-expiry behavior: rows with
  `expires_at <= requested_timestamp` are invisible for timestamp reads.
- Added a local `BranchTimestampCoverage` proof hook and typed
  `InsufficientTimestampHistory` error. Default read views use unknown coverage,
  so observed `timestamp_min` alone does not turn a best-effort miss into an
  insufficient-history failure.

### Deferred

- Durable retained-history proof publication and recovery are still later
  retention/lifecycle work.
- Commit timeline lookup from application timestamp to branch frontier is still
  outside L6G.
- TTL cleanup, TTL-based physical deletion, and branch compaction safety remain
  L6J/L8 territory.
- Materialization and reachability behavior over timestamp-visible inherited
  rows remain L6H/L6I.
- Snapshot row install remains L6K.

### Tests Ported Or Added

- Enabled `BranchReadBound::AtTimestamp` in `BranchReadView::read_point`,
  `scan_prefix`, and `scan_range`.
- Added central selected-row visibility logic so point reads and scans share
  timestamp-bound, tombstone, and TTL behavior.
- Added direct branch tests for timestamp filtering, non-monotonic commit
  timestamps, exact timestamp inclusion, tombstones at timestamp, TTL
  before/exact/after expiry, `Timestamp::MAX` as a real far-future expiry,
  frozen and owned-table timestamp reads, timestamp scans, timestamp scan
  boundary/empty-result/key-space isolation, inherited timestamp scans after
  source-to-child key rewriting, timestamp-pinned read views, inherited
  timestamp plus fork-version gates, child-local expired-row inherited
  suppression, child-local put/tombstone inherited shadowing, nearest inherited
  exact-tie selection, source-mutation isolation for captured inherited
  timestamp views, and known-insufficient timestamp coverage.
- Updated wrong-branch timestamp tests to assert wrong-branch validation still
  runs before payload inspection while valid timestamp reads now succeed.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` with generated
  timestamp categories for active/frozen/owned point reads, prefix/range scans,
  inherited scan rewrites, pinned timestamp views, TTL boundaries,
  `Timestamp::MAX` expiry, tombstones, tombstone-after-bound non-shadowing,
  scan boundary/empty-result/key-space isolation, non-monotonic timestamps,
  inherited timestamp reads, inherited fork gates, inherited child-local
  put/tombstone shadowing, nearest inherited exact-tie selection, unknown
  coverage, and insufficient-history rejection.
- Updated `crates/storage-next/tests/branch_lsm_properties.rs` so the property
  harness requires every L6G generated category to be exercised.

### Sensitivity Probes

- `branch_read_view_timestamp_reads_filter_by_timestamp_then_commit_version`
  catches sorting timestamp reads by timestamp instead of commit-version row
  order.
- `branch_read_view_timestamp_tombstones_suppress_fallthrough` and
  `branch_read_view_timestamp_ttl_boundaries_suppress_fallthrough` catch
  exact-expiry inclusivity drift, wall-clock TTL assumptions, selected expired
  row fallthrough, and tombstone fallthrough.
- `branch_read_view_timestamp_max_expiry_is_far_future_not_no_expiry` catches
  conflating the `Timestamp::MAX` expiry value with the `Timestamp::EPOCH`
  no-expiry sentinel.
- `branch_read_view_timestamp_reads_cover_frozen_and_owned_sources` catches
  timestamp point reads accidentally working only for active rows.
- `branch_read_view_timestamp_scans_apply_tombstone_and_ttl_per_key` catches
  timestamp scan grouping/output drift and per-key visibility fallthrough.
- `branch_read_view_timestamp_scans_preserve_bounds_and_empty_results` catches
  open/closed timestamp range drift and empty-result fallback.
- `branch_read_view_timestamp_scans_preserve_key_spaces` catches leakage across
  named or storage-space key domains.
- `branch_inherited_timestamp_scans_rewrite_source_keys_before_grouping` catches
  inherited scan filtering against child scan bounds before branch-id rewrite.
- `branch_read_view_timestamp_views_are_pinned_across_later_mutations` and
  `branch_inherited_timestamp_view_is_pinned_after_source_mutation` catch
  timestamp view instability after later branch-local or source-branch changes.
- `branch_inherited_timestamp_reads_apply_timestamp_and_fork_gates` catches
  inherited rows above fork versions, inherited rows above timestamp bounds,
  and child-local expired-row fallback to inherited values.
- `branch_inherited_timestamp_reads_pick_nearest_layer_for_exact_ties` catches
  exact-tie instability across inherited layers.
- `branch_inherited_timestamp_reads_apply_local_put_and_tombstone_shadows`
  catches child-local put/tombstone fallback to inherited values.
- `branch_timestamp_coverage_rejects_only_proven_insufficient_history` catches
  inferring insufficient history from observed timestamp ranges without a
  coverage proof, and catches payload leakage in coverage errors.
- The generated branch-LSM property harness asserts nonzero coverage for every
  L6G timestamp/TTL category, preventing the timestamp path from regressing to
  a file-presence placeholder.

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
- Legacy-retained: old wall-clock TTL wrappers, old segmented as-of paths, and
  old `VersionedValue` based history remain because current storage still
  depends on them.
- Follow-up: L6H/L6I/L6J/L6K/L8 will retire more old branch behavior after
  materialization, durable reachability, compaction safety, snapshot install,
  and lifecycle replacement slices land in storage-next.

## L6H: Materialization Mechanics

### Current Files Read

- `crates/storage/src/segmented/mod.rs`
- `crates/storage/src/segmented/tests/materialize.rs`
- `crates/storage/src/segmented/tests/concurrency.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/tests.rs`
- `crates/storage-next/src/testkit/branch_lsm.rs`
- `crates/storage-next/tests/branch_lsm_properties.rs`
- `crates/storage-next/tests/branch_lsm_source_guard.rs`

### Behavior Preserved

- Preserved materialization as a physical ownership transition: inherited
  source rows are rewritten into child-owned immutable table rows.
- Preserved fork-version gating. Rows above the inherited layer fork version
  are not materialized.
- Preserved source-to-child branch-id rewriting while keeping logical space,
  storage-space id, user key, commit version, commit timestamp, expiry,
  tombstone flag, and value bytes unchanged.
- Preserved read parity across latest, version-bounded, timestamp-bounded, and
  history reads before and after materialization.
- Preserved idempotent stale-state behavior for already-materialized layers and
  no-table behavior for empty inherited layers.
- Preserved exact duplicate suppression for byte-identical rewritten rows
  already represented by a higher-precedence child-local or nearer inherited
  source, while retaining same-internal-key rows whose timestamps, expiry,
  tombstone bit, or value bytes differ.

### Intentional V1 Changes

- Did not port disk segment creation, filesystem directories, manifest writes,
  publish-health latching, refcount decrements, or orphan file GC into L6H.
  Those are L4/L6I/L8 responsibilities in storage-next.
- Did not port old product `Value`, `Key`, `Namespace`, `TypeTag`, or
  `VersionedValue` vocabulary.
- Materialization does not perform broad cleanup. It keeps retained historical
  rows that may be visible through `getv`, timestamp reads, or storage history.
  Compaction/retention proof work owns later pruning.
- Replacement tables are built through the L5 immutable table builder/reader
  path and installed as branch-owned L0 tables.
- Recovery is represented as storage-owned in-memory outcome facts, not durable
  manifest state.

### Deferred

- Durable materialization intent publication, ambiguous publish-window
  reconciliation, and crash recovery are L8 lifecycle work.
- Durable branch/table reachability payloads and shared table release facts
  remain L6I/L8 work.
- Branch compaction and retention-proof based row pruning remain L6J/L8 work.
- Snapshot row install remains L6K.
- Dedicated materialization fuzz inventory remains L6L closeout work.

### Tests Ported Or Added

- Added `BranchMaterializationRequest`, `BranchMaterializationOutcome`, and
  `BranchMaterializationRecovery` storage-owned facts.
- Added `BranchLocalState::materialize_inherited_layer` to collect retained
  inherited rows, rewrite them to the child branch, build L5 replacement
  tables, install those tables into L0, and remove the inherited layer from new
  read views.
- Added direct branch tests for retained-history preservation, latest/getv/as-of
  parity, history parity, post-fork row exclusion, byte-identical duplicate
  suppression, same-internal-key fact divergence, scan parity, tombstone/TTL
  preservation, pinned view isolation, output splitting, empty-layer
  materialization, already-materialized no-op behavior, request/status
  validation, edge row/table fact preservation, layer-order preservation, and
  invalid output identity rejection.
- Extended the generated branch-LSM testkit with materialization attempts,
  success/empty/idempotent counters, materialized row/table counters, skipped
  post-fork/duplicate counters, read-parity counters, tombstone/TTL
  preservation counters, owned-table source checks, pinned-view isolation, and
  invalid-request rejection counters.
- Updated `branch_lsm_source_guard.rs` so materialization is no longer treated
  as premature behavior while compaction and snapshot install remain guarded.

### Sensitivity Probes

- `branch_materialization_rewrites_retained_rows_without_cleanup` catches
  branch-id rewrite drift, row fact loss, inherited-layer removal without
  replacement rows, and broad cleanup that would drop retained history.
- `branch_materialization_skips_post_fork_rows_and_exact_duplicates_only`
  catches missing fork-version filtering, exact duplicate resurrection, and
  over-broad same-key pruning that would remove older visible versions.
- `branch_materialization_handles_empty_and_already_materialized_layers` catches
  non-idempotent replay behavior, empty table creation, and invalid output
  identity acceptance.
- `branch_materialization_accepts_materializing_layer_status` catches retry
  drift where a materializing layer no longer produces child-owned replacement
  rows.
- `branch_materialization_rejects_bad_request_without_mutation` and
  `branch_materialization_rejects_unavailable_same_source_and_invalid_descriptors`
  catch invalid materialization requests mutating branch state or silently
  accepting corrupt inherited metadata.
- `branch_materialization_preserves_edge_row_facts_and_table_facts` catches
  loss of empty values, storage-owned key facts, binary keys, expiry facts, and
  replacement table facts.
- `branch_materialization_preserves_layer_order_when_deep_layer_materialized_first`
  and
  `branch_materialization_preserves_nearest_and_history_after_all_layers_materialize`
  catch nearest-layer precedence drift and deep-layer materialization changing
  remaining inherited-layer order.
- `branch_lsm_source_has_no_premature_behavior_entrypoints` catches
  accidental L6J/L6K entrypoint creep while allowing the L6H-owned
  materialization method.

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
- Legacy-retained: old disk-backed `materialize_layer`, manifest status
  publication, shared segment refcount decrements, and materialization GC remain
  because current storage still depends on them.
- Follow-up: L6I/L6J/L6K/L8 will retire more old branch behavior after durable
  reachability, branch compaction safety, snapshot install, and lifecycle
  recovery land in storage-next.

## L6I: Reachability And Shared Table Refs

### Current Files Read

- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage/src/segmented/tests/fork.rs`
- `crates/storage/src/segmented/tests/concurrency.rs`
- `crates/storage/src/segmented/tests/quarantine_reconciliation.rs`
- `crates/storage/src/manifest.rs`
- `crates/storage-next/src/branch/facts.rs`
- `crates/storage-next/src/branch/state.rs`
- `crates/storage-next/src/branch/read.rs`
- `crates/storage-next/src/branch/tests.rs`
- `crates/storage-next/src/testkit/branch_lsm.rs`
- `crates/storage-next/tests/branch_lsm_properties.rs`
- `crates/storage-next/tests/branch_lsm_source_guard.rs`

### Behavior Preserved

- Preserved the old shared-ref deletion barrier as a storage-local table
  reachability model: a table is not releasable while any branch snapshot,
  inherited layer, materializing layer, or runtime registry entry still refers
  to it.
- Preserved the source-of-truth rule that durable reachability facts beat the
  runtime registry. A registry/aggregate disagreement is reported as protected
  state, not as a release candidate.
- Preserved deterministic shared-reference rebuilds. `BranchReachabilitySnapshot`
  and `BranchReachabilityAggregate` sort table refs by stable table identity and
  reference facts, so insertion order does not affect release planning.
- Preserved fork sharing semantics: forked children reference source tables
  through inherited reachability facts rather than copying rows or minting new
  table identities.
- Preserved materialization safety: source inherited refs remain protected while
  a layer is materializing and become release candidates only after replacement
  child-owned reachability is visible in the branch snapshot.
- Preserved replacement provenance for materialized tables so L8 can distinguish
  ordinary child-owned tables from materialization replacement outputs.

### Intentional V1 Changes

- Did not port filesystem segment refcount files, async locks, object deletion,
  quarantine, manifest publication, or GC into L6I. L6I emits in-memory
  storage-owned facts and release plans; L4/L8 own durable publication and
  physical deletion.
- The runtime `SharedTableRegistry` is an accelerator over decoded reachability
  snapshots. It tracks registered branch snapshots to reject duplicate or stale
  unregister calls, supports atomic same-branch snapshot replacement, but it is
  not a durable proof of release safety.
- Release planning is table-identity based. It reports `StillReachable`,
  `RuntimeReferenced`, or `RegistryDisagreement` protection reasons, and emits
  releasable table identities only when both aggregate reachability and the
  optional runtime registry have no remaining refs. Any positive count mismatch
  between aggregate reachability and the runtime registry is classified as
  `RegistryDisagreement`.
- Branch clear/delete is represented as a release plan over removed refs. Product
  branch lifecycle policy, object deletion, and quarantine are intentionally
  deferred.

### Deferred

- Durable branch/table reachability manifests and crash-window reconciliation
  remain L8 work.
- Physical table deletion, quarantine handoff, and retention scheduling remain
  L8 work.
- Branch compaction release proofs remain L6J/L8.
- Snapshot install reachability integration remains L6K/L8.
- Cross-process shared-reference leases are outside L6.

### Tests Ported Or Added

- Added `BranchTableRef`, `BranchTableReferenceKind`,
  `BranchReachabilitySnapshot`, `BranchReachabilityAggregate`,
  `SharedTableRegistry`, `BranchReleasePlan`, and protection reason facts.
- Added `BranchLocalState::reachability_snapshot`, covering owned immutable
  tables, active inherited layers, materializing inherited layers, and
  unavailable-layer rejection while excluding active/frozen mutable rows.
- Added direct branch tests for deterministic reachability ref sorting,
  validation, owned/inherited/materializing/replacement classifications,
  aggregate shared-table detection, registry rebuild/unregister behavior,
  atomic registry snapshot replacement, duplicate unregister rejection, release
  candidates, runtime protection, zero-count and positive-count registry
  disagreement, fork reachability, replacement provenance,
  materializing-layer protection, empty/inherited branch clear release,
  multi-layer materialization release, and deterministic rebuild from decoded
  reachability refs.
- Extended the generated branch-LSM testkit with reachability snapshot,
  owned/inherited/materializing ref, aggregate rebuild, shared detection,
  release candidate, protected release, registry rebuild/unregister,
  disagreement, fork rollback, materialization release, branch clear release,
  deterministic ordering, and invalid reachability counters.
- Updated the branch source guard to recognize L6I-owned reachability entrypoints
  while continuing to reject upper-layer imports, backend IO, product DTO
  vocabulary, old segment-ref registry vocabulary, compaction, and snapshot
  install entrypoints.

### Sensitivity Probes

- `branch_reachability_fact_types_are_deterministic_and_validated` catches
  nondeterministic table ref ordering, duplicate refs, same-branch inherited
  refs, owner-branch mismatches, and lost owned/inherited counts.
- `branch_reachability_snapshot_tracks_owned_and_inherited_tables_only` catches
  mutable active/frozen rows leaking into durable table reachability and
  materializing layers failing to retain source refs.
- `branch_reachability_aggregate_registry_and_release_plans_are_safe` catches
  shared-table early release, stale runtime ref handling, registry/aggregate
  disagreement becoming releasable, and duplicate unregister count corruption.
- `branch_reachability_registry_snapshot_replacement_updates_counts_atomically`
  catches stale snapshot replacement leaving old refs behind, missing new refs,
  duplicate registration, and stale unregister underflow.
- `branch_reachability_release_plans_cover_empty_clear_and_inherited_refs`
  catches empty clear drift, parent clear releasing child-inherited tables,
  inherited-ref clear plans losing removed-ref kind, and active/frozen rows
  leaking into durable reachability facts.
- `branch_reachability_materialization_release_is_limited_to_removed_layer`
  catches materialization release plans dropping nearer-layer refs or retaining
  removed deep-layer source refs after replacement reachability is visible.
- `branch_reachability_rebuild_from_decoded_refs_is_deterministic_and_blocks_mismatches`
  catches nondeterministic decoded-fact rebuilds, duplicate branch snapshots,
  and positive registry/aggregate count disagreements becoming releasable.
- `branch_lsm_property_harness_runs_scaffold_contract` now fails unless all L6I
  reachability counters are exercised by generated cases.
- `branch_lsm_source_guard_catches_required_forbidden_terms` catches accidental
  resurrection of old `SegmentRefRegistry` vocabulary.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`

### Retirement

- Deleted: none.
- Legacy-retained: old disk-backed segment refcount registry, shared segment
  deletion barriers, and manifest publication remain because current storage
  still depends on them.
- Follow-up: L6J/L6K/L8 will retire more old branch behavior after branch
  compaction safety, snapshot install, durable reachability publication, and
  lifecycle recovery land in storage-next.

## L6J: Branch Compaction Integration

### Current Files Read

- `crates/storage/src/segmented/compaction.rs`
- `crates/storage/src/segmented/tests/compaction.rs`
- `crates/storage/src/segmented/tests/concurrency.rs`
- `crates/storage/src/segmented/ref_registry.rs`
- `crates/storage-next/src/table/compaction.rs`
- `crates/storage-next/src/branch/{state.rs,read.rs,facts.rs,error.rs}`
- `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6j-branch-compaction-integration-test-plan.md`

### Behavior Preserved

- Preserved compaction as a branch-local table replacement: selected immutable
  tables are merged through the table runtime, replacement tables are installed,
  and old table refs are reported for later lifecycle cleanup.
- Preserved L0 overlap semantics. L0 keep-all compaction may rewrite overlapping
  L0 tables without changing latest, version-bounded, timestamp-bounded, or
  history reads.
- Preserved next-level overlap inclusion for L0-to-L1 and nonzero-level
  compactions. Candidate selection now uses physical-key overlap so different
  versions of the same logical key are compacted together even when internal-key
  ranges differ by commit version.
- Preserved the old safety split between replacement and cleanup: L6J emits
  removed refs but does not delete objects, publish manifests, or treat
  compaction as a release proof by itself.
- Preserved concurrent-state safety at the in-memory boundary by revalidating
  candidate refs immediately before building sources and again before install.

### Intentional V1 Changes

- Did not port old score-based background scheduling, async task loops,
  filesystem segment paths, manifest writes, rate limiting, environment
  logging, or physical segment deletion.
- Did not port pruning heuristics. L6J accepts only keep-all compaction for now;
  old-version, tombstone, and TTL pruning requests are rejected as typed
  `InvalidCompaction` errors until a later slice supplies explicit retention
  proofs.
- Rebuilt compaction over L5 `TableCompactor` and storage-next branch-owned
  table refs rather than old segment ids.
- Kept active rows, frozen rows, inherited source tables, and materializing
  source refs out of direct compaction inputs. Materialized replacement tables
  are treated as ordinary owned-like tables for future compaction.

### Deferred

- Retention-proof-backed old-version, tombstone, and TTL pruning.
- Score-based scheduling, compaction picking policy, and background execution.
- Durable branch manifest publication and crash-window reconciliation.
- Physical table-object deletion, quarantine handoff, and release execution.
- Snapshot row install integration remains L6K.

### Tests Ported Or Added

- Added branch-local tests for unsafe pruning rejection, L0 keep-all compaction,
  L0-to-L1 overlap inclusion, nonzero-level promotion, explicit no-op plans,
  stale-plan revalidation, and output-identity collision rejection without
  mutation.
- Expanded the direct L6J suite with invalid level/table-index requests,
  mutable/frozen/inherited source exclusion, keep-all read parity across
  active/frozen/inherited rows, tombstones, TTL rows, high-bit keys, and split
  outputs, materialized-replacement compaction inputs, L5 build-failure
  atomicity, registry-disagreement protection, and branch-clear release facts
  for compaction outputs.
- Extended the generated branch-LSM testkit with compaction no-op, L0,
  L0-to-L1, nonzero-level, keep-all parity, output split, stale-candidate,
  unsafe-pruning, release-candidate, protected-release, and invalid-request
  counters.
- Extended `branch_lsm_properties.rs` so every L6J generated compaction counter
  must be nonzero.
- At the L6J point, updated the branch source guard to mark compaction
  planning/install as L6J owned while still rejecting snapshot install,
  backend IO, lifecycle, and product DTO vocabulary. L6K narrows that guard
  again for row-native snapshot install.

### Sensitivity Probes

- `branch_compaction_rejects_pruning_policies_without_mutation` catches
  accidental row-dropping policy enablement before retention proofs exist.
- `branch_compaction_l0_keep_all_installs_replacement_and_preserves_pinned_view`
  catches keep-all read-parity loss and pinned read-view invalidation across an
  L0 rewrite.
- `branch_compaction_l0_to_level_one_includes_overlaps_and_preserves_non_overlaps`
  catches missing physical-key overlap inclusion and accidental deletion of
  unrelated L1 tables.
- `branch_compaction_nonzero_level_promotes_overlapping_tables_only` catches
  nonzero-level overlap omission and target-level ordering drift.
- `branch_compaction_install_revalidates_stale_plan_before_mutation` catches
  stale candidate refs being applied after a concurrent branch-state mutation.
- `branch_compaction_rejects_output_identity_collision_without_mutation` catches
  replacement output ids that would alias a surviving branch-owned table.
- `branch_compaction_rejects_inherited_output_identity_collision_without_mutation`
  catches replacement output ids that would alias an inherited reachable table.
- `branch_compaction_candidates_exclude_mutable_and_inherited_sources` catches
  active, frozen, inherited, or value-bearing facts entering candidate refs.
- `branch_compaction_keep_all_read_parity_covers_mutable_inherited_ttl_and_split_outputs`
  catches keep-all parity drift across read modes, tombstone filtering, TTL
  visibility, high-bit keys, active/frozen/inherited rows, and split outputs.
- `branch_compaction_materialized_replacements_are_inputs_but_outputs_are_plain_owned_refs`
  catches rejected materialized-replacement inputs and replacement provenance
  leaking into ordinary compaction outputs.
- `branch_compaction_build_failure_preserves_state_without_partial_outputs`
  catches partial output visibility after an L5 output-build failure.
- `branch_compaction_release_facts_cover_shared_refs_disagreement_and_clear_outputs`
  catches registry-disagreement misclassification and branch-clear release of
  stale removed refs instead of visible output refs.
- The generated branch-LSM property harness catches missing generated coverage
  for compaction no-ops, candidate shapes, keep-all read parity, output splits,
  stale candidates, unsafe pruning rejection, and release/protection facts.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch_compaction`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
- `cargo fmt --package strata-storage-next --check`
- `git diff --check`

### Retirement

- Deleted: none.
- Legacy-retained: old segmented compaction and compaction tests remain because
  current storage still depends on them.
- Follow-up: L6K/L6L/L8 will close snapshot install, full L6 conformance, and
  durable compaction lifecycle orchestration.

## L6K: Snapshot Row Install

### Current Files Read

- `crates/storage/src/durability/decoded_snapshot_install.rs`
- `crates/storage-next/src/row/mod.rs`
- `crates/storage-next/src/table/{builder.rs,reader.rs,key.rs,facts.rs}`
- `crates/storage-next/src/branch/{state.rs,read.rs,facts.rs,error.rs}`
- `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
- `docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L6/l6k-snapshot-row-install-test-plan.md`

### Behavior Preserved

- Preserved decoded-row snapshot install as a generic storage boundary. L6K
  receives storage-next `StorageRow` values, not engine snapshot DTOs.
- Preserved full preflight before mutation. Missing targets, duplicate branch
  groups, branch-id mismatches, duplicate internal keys, unsorted groups, and
  non-empty targets are rejected before branch-state visibility changes.
- Preserved multi-branch all-or-nothing behavior by staging every target branch
  state and swapping the caller-owned branch set only after all table artifacts
  build, decode, install, and emit reachability facts.
- Preserved tombstones, timestamps, TTL facts, empty values, high-bit keys, and
  multiple versions as normal retained storage rows.

### Intentional V1 Changes

- Replaced old `DecodedSnapshotEntry`/`TypeTag`/primitive value vocabulary with
  row-native `StorageRow` groups keyed by storage `BranchId`.
- Kept the V1 target policy conservative: existing targets must be empty, and
  missing targets are rejected unless the request explicitly supplies a create
  policy and `BranchRuntimeConfig`.
- Built branch-owned L0 table artifacts through L5 and reopened them before
  branch-state mutation. L6K does not publish table objects or manifests.
- Generated output identities from a caller seed, branch id, table index, and
  row fingerprint so same-seed installs across branches do not alias.

### Deferred

- Durable snapshot byte decoding and section routing remain L3/L8 work.
- Durable table-object publication, branch manifest publication, and
  crash-window reconciliation remain L4/L8 work.
- Product restore semantics, branch deletion/clear orchestration, and
  StrataHub import/export remain above L6.
- Fuzz inventory remains part of L6L closeout; L6K now contributes generated
  branch-LSM property counters through the shared scaffold harness.

### Tests Ported Or Added

- Added direct branch tests for empty install no-op, missing branch rejection,
  explicit missing-branch creation, non-empty target rejection, row branch
  mismatch rejection, duplicate internal-key rejection, unsorted group
  rejection, output-identity collision rejection, multi-branch install,
  post-install latest/version reads, and reachability facts.
- Extended `crates/storage-next/src/testkit/branch_lsm.rs` with generated
  snapshot install cases for empty no-op, single-branch install, multi-branch
  install, reject-vs-create missing branch policy, non-empty target rejection,
  empty group rejection, duplicate group rejection, branch mismatch rejection,
  duplicate/unsorted row rejection, output identity collision rejection,
  table-build failure atomicity, latest/version/as-of/history/prefix/range
  parity, tombstone/TTL preservation, pinned-view isolation, reachability,
  empty user keys, high-bit keys, empty values, larger valid values,
  `Timestamp::MAX`, shared user keys across branches, and alternate storage
  spaces.
- Updated `crates/storage-next/tests/branch_lsm_properties.rs` so the generated
  branch-LSM property harness requires every L6K snapshot install counter to be
  nonzero.
- Updated the branch source guard to treat row-native snapshot install as an
  L6K-owned entrypoint while continuing to reject backend IO, L4 services,
  lifecycle, old storage DTOs, product values, and engine vocabulary.

### Sensitivity Probes

- `branch_snapshot_install_rejects_missing_and_non_empty_targets_without_mutation`
  catches accidental implicit branch creation or merge-into-live-branch
  behavior.
- `branch_snapshot_install_rejects_invalid_rows_before_any_branch_mutates`
  catches branch-id mismatch, duplicate key, and unsorted input acceptance.
- `branch_snapshot_install_rejects_output_identity_collisions_without_mutation`
  catches table identity aliasing against already reachable branch tables.
- `branch_snapshot_install_builds_l0_tables_and_preserves_reads` catches table
  build/open omissions, per-branch output aliasing, read-parity drift, and
  reachability fact omission.
- `branch_lsm_property_harness_runs_scaffold_contract` catches missing
  generated coverage for snapshot install validation, atomicity, read parity,
  tombstone/TTL handling, pinned-view isolation, reachability, and row-native
  boundary cases.
- `branch_lsm_source_guard` catches L6K crossing into snapshot codecs, backend
  IO, durable services, lifecycle, engine, old storage DTO, or product-value
  vocabulary.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch_snapshot`
- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`

### Retirement

- Deleted: none.
- Legacy-retained: old decoded snapshot install remains because current storage
  still depends on it.
- Follow-up: L6L closeout adds fuzz hooks and the final conformance ledger.

## L6L: L6 Conformance Closeout

### Current Files Read

- `crates/storage-next/src/branch/`
- `crates/storage-next/src/testkit/branch_lsm.rs`
- `crates/storage-next/src/testkit/mod.rs`
- `crates/storage-next/tests/branch_lsm_properties.rs`
- `crates/storage-next/tests/branch_lsm_source_guard.rs`
- `crates/storage-next/fuzz/Cargo.toml`
- `crates/storage-next/fuzz/fuzz_targets/`
- `crates/storage-next/fuzz/corpus/`
- `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
- `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
- all L6A through L6K slice plans and this porting log

### Behavior Preserved

- Preserved the L6A through L6K storage contract as an in-memory,
  branch-isolated LSM runtime. L6 still owns branch-local rows, pinned read
  views, inherited layers, timestamp/TTL filtering, materialization mechanics,
  reachability facts, branch compaction state transitions, and row-native
  snapshot install.
- Preserved the L6 boundary: no commit allocator, lifecycle scheduler, backend
  handle, object layout constructor, durable service call, product DTO,
  StrataHub remote concept, or public branch API is introduced by closeout.
- Preserved the generated property harness as the broad conformance route. L6L
  adds closeout inventory around it rather than replacing it.
- Preserved old-code evidence in the log. Every old branch, merge iterator,
  memtable, segmented compaction, and decoded snapshot-install behavior is
  marked preserved, intentionally changed, retired, or deferred.

### Intentional V1 Changes

- Added three dedicated branch-LSM fuzz contracts:
  `check_branch_lsm_reads_contract`,
  `check_branch_lsm_inheritance_contract`, and
  `check_branch_lsm_install_contract`.
- Added `check_branch_lsm_reference_model_contract`, which replays generated
  append/rotate/L0-install operation scripts against both `BranchLocalState`
  and an independent row-list `ModelBranch` for latest/getv/as-of/history and
  scan parity.
- Added `check_branch_lsm_fault_window_contract`, which exercises L6-local
  failed preflight windows and verifies duplicate append, wrong-branch install,
  materialization identity collision, and snapshot table-build failures leave
  state unchanged.
- Tightened materialization retry semantics so a retry that finds replacement
  tables already visible removes the source layer by stable
  `(source_branch_id, fork_version)` identity instead of by the original array
  index.
- Tightened branch facts so inherited fork-version descriptors no longer
  synthesize `max_commit_version` without row timestamps; observed max version
  now comes from actual retained rows.
- Wired canonical captured read views to complete-since coverage derived from
  observed timestamp facts, making insufficient-history errors reachable
  without a test-only override.
- Added three matching fuzz targets: `branch_lsm_reads`,
  `branch_lsm_inheritance`, and `branch_lsm_install`.
- Added checked-in seed corpora for those branch fuzz targets.
- Added `branch_lsm_closeout.rs` to verify the closeout inventory: generated
  counters, source-guard categories, fuzz target registration, dedicated
  contract usage, non-empty corpora, and porting-log coverage.
- Strengthened the branch source guard so L6 also rejects `crate::object`,
  StrataHub, remote-tracking, provider-capability, dataset, and branch-name
  product vocabulary in production branch code.

### Deferred

- commit-version allocation belongs to L7.
- commit conflict validation belongs to L7.
- WAL-before-visible discipline belongs to L7/L8.
- durable branch manifest publication belongs to L8.
- durable recovery and checkpoint orchestration belong to L8.
- compaction scheduling, retention scheduling, physical table deletion,
  quarantine handoff, and release execution belong to L8.
- public branch naming, product branch workflows, restore/revert/cherry-pick,
  and `Versioned<T>` product mapping belong to L9/engine.
- branch-registry workflows such as duplicate branch create, fork on a missing source branch,
  and fork-at-history requests belong to the L7/L9 branch
  registry/API surface. L6 exposes `BranchLocalState`, not a named branch
  catalog, so these cases are deliberately not implemented here.
- branch clear/delete APIs, including pinned-view behavior across public
  clear/delete operations, belong to the L8/L9 lifecycle/API surface. L6I
  exposes release planning facts but does not expose product lifecycle
  operations.
- visible-but-not-durable materialization windows, durable materialization
  recovery records, backend fault reconciliation, and richer recovery variants
  belong to L8 durable orchestration.
- cross-fork materialization provenance diagnostics are deferred until L8
  defines durable materialization recovery facts; L6 keeps replacement refs
  reachable and identity-safe but does not preserve grand-ancestor diagnostic
  chains.
- StrataHub push, pull, clone, sync, remote-tracking refs, and provider
  capability discovery belong above storage-next.
- query planning and secondary indexing belong above the L6 branch LSM.
- post-V1 retained-history proofs may allow older fork-version requests and
  safe old-version/tombstone/TTL pruning.

### Tests Ported Or Added

- Added `check_branch_lsm_reads_contract` for latest/getv/as-of/history,
  prefix/range scans, row-chain visibility, timestamp/TTL reads, and
  branch-owned immutable read paths.
- Added `check_branch_lsm_inheritance_contract` for fork gates, inherited key
  rewriting, child-local shadowing, inherited timestamp reads,
  materialization parity, and reachability facts.
- Added `check_branch_lsm_install_contract` for branch-owned table install,
  compaction install, snapshot row install, and release/protection facts.
- Added `check_branch_lsm_reference_model_contract` for generated operation
  scripts checked against an independent `ModelBranch` rather than hand-written
  constants.
- Added `check_branch_lsm_fault_window_contract` for L6-local no-mutation
  windows after failed validation or staged install/build work.
- Added `branch_lsm_reads`, `branch_lsm_inheritance`, and
  `branch_lsm_install` fuzz target registrations plus seed corpora.
- Added `branch_lsm_closeout.rs` so closeout fails if fuzz targets are missing,
  targets call only `check_branch_lsm_scaffold_contract`, generated counters
  are not asserted by `branch_lsm_properties.rs`, source guards lose required
  categories, or this log misses an L6A through L6L section.
- Extended `branch_lsm_source_guard.rs` with product/remote vocabulary and
  `crate::object` probes.

### Sensitivity Probes

- `branch_lsm_closeout_generated_harness_exposes_every_counter` catches new
  generated counters that are not required by the property harness.
- `branch_lsm_closeout_fuzz_inventory_matches_branch_targets` catches missing
  fuzz target files, missing `fuzz/Cargo.toml` entries, missing seed corpora,
  or placeholder targets that call the broad scaffold contract instead of a
  dedicated branch-LSM contract.
- `branch_lsm_closeout_source_guard_suite_covers_required_boundary_categories`
  catches source-guard drift around commit/lifecycle/engine/backend/object/
  layout/service imports, filesystem/environment/time APIs, product DTOs,
  StrataHub vocabulary, and public branch API leakage.
- `branch_lsm_closeout_porting_log_records_full_l6_ledger` catches closeout
  documentation drift for L6A through L6L sections, branch fuzz coverage,
  mandatory commands, and owner-layer deferrals.
- The direct/generated L6 tests and closeout ledger structurally enforce the
  L6L sensitivity probes for tombstone fallthrough, TTL bound handling,
  fork-version gates, child-local shadowing, inherited materialization ordering,
  shared table release protection, unsafe old-version/tombstone/TTL pruning,
  snapshot branch-id mismatch rejection, duplicate snapshot row rejection, and
  no-mutation behavior after failed preflight.
- Direct branch-owned installs now reject table identity collisions against all
  reachable owned and inherited tables, nonzero-level overlap checks use
  physical-key ranges, and snapshot `from_rows` normalizes each branch group by
  internal key before validation.
- `check_branch_lsm_reference_model_contract` catches correlated expectation
  bugs in generated own-branch read tests by comparing production output to a
  separately implemented `ModelBranch` for operation-script replay.
- Materialization generated coverage now checks the post-materialization state
  against `ModelBranch` in addition to the before/after parity check, and it
  includes the child-owned immutable same-internal-key rejection case.
- `check_branch_lsm_fault_window_contract` catches state mutation during
  L6-local failed preflight windows without introducing backend or lifecycle
  responsibilities.
- `branch_materialization_retry_removes_layer_when_replacements_are_already_visible`
  catches retry paths that self-collide on previously visible replacement
  identities or remove an inherited layer by stale index instead of stable
  source identity.
- `branch_fork_rejects_inherited_only_source_without_own_applied_version`
  catches synthetic fork versions derived only from inherited descriptors.
- `branch_materialization_handles_empty_and_already_materialized_layers`
  catches empty inherited layers that synthesize a max commit version without
  timestamp facts.
- `branch_timestamp_coverage_rejects_only_proven_insufficient_history` catches
  canonical captured views that leave timestamp coverage unknown after row
  facts prove an earliest available timestamp.
- The branch source guard itself catches accidental introduction of
  `VersionedValue`, product `Value`/`Key`, `StrataHub`, `RemoteTrackingRef`,
  `ProviderCapability`, `Dataset`, `BranchName`, backend calls, L4 service
  calls, `crate::commit`, `crate::lifecycle`, `crate::backend`,
  `crate::service`, wall-clock reads, and public branch exports in production
  L6 code.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch`
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard`
- `cargo test -p strata-storage-next --locked --test branch_lsm_closeout`
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties`
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties`
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked`
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings`
- `cargo test -p strata-storage-next --locked`
- `cargo fmt --package strata-storage-next --check`
- `git diff --check`
- Optional/manual: `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60`
- Optional/manual: `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60`
- Optional/manual: `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60`

### Retirement

- Deleted: none.
- Legacy-retained: old branch/segmented storage remains until the architecture
  rewrite swaps higher layers onto storage-next.
- Follow-up: L7 can commit into branch state; L8 can publish/recover branch
  manifests and execute lifecycle cleanup; L9/engine can expose product branch
  workflows and StrataHub integration without adding those responsibilities to
  L6.

## L6M: Assurance Depth

### Scope

L6M closes the assurance-depth gap left after the first L6 closeout. It does
not add a new branch-runtime feature. It strengthens the generated and fuzz
routes so inheritance, materialization, snapshot install, and compaction install
are checked against an independent model rather than only fixed fixtures or
production-derived before/after values.

### Implemented

- Added an inheritance-aware `ModelBranchStore` in
  the split `crates/storage-next/src/testkit/branch_lsm` testkit module.
- Split `crates/storage-next/src/testkit/branch_lsm.rs` into a small module
  front door plus `crates/storage-next/src/testkit/branch_lsm/scaffold.rs` so
  the original path is no longer a multi-thousand-line implementation file.
- Added `ModelInheritedLayer` with source branch, fork version, and source-row
  facts so generated checks can model inherited visibility independently.
- Added `assert_model_store_read_surface`, which compares production latest,
  versioned, timestamp, history, prefix, and range reads against the model.
- Added `check_branch_lsm_inheritance_model_contract`, covering inherited layer
  reads, fork-version gates, branch-id rewriting, child-local put shadowing,
  child-local tombstone shadowing, chained ancestry, and materialization read
  parity.
- Added `check_branch_lsm_install_model_contract`, covering snapshot install and
  compaction install parity against the model.
- Added script-driven model cases under those contracts so generated property
  checks exercise decoded operation streams in addition to fixed edge fixtures.
- Wired the model-backed contracts into the existing
  `check_branch_lsm_inheritance_contract` and
  `check_branch_lsm_install_contract` routes.
- Added `inheritance_model_cases` and `install_model_cases` outcome counters
  and wired them into `check_branch_lsm_scaffold_contract`, so
  `branch_lsm_properties.rs` fails if these model routes stop running.
- Strengthened `branch_lsm_properties.rs` so the generated property harness
  requires the model-backed contracts and model symbols.
- Strengthened `branch_lsm_closeout.rs` so closeout requires the model-backed
  routes and checks every branch fuzz corpus has at least two non-empty seeds
  plus a named scenario script.
- Added scenario corpus seeds:
  - `fuzz/corpus/branch_lsm_reads/basic-script`
  - `fuzz/corpus/branch_lsm_reads/range-history-script`
  - `fuzz/corpus/branch_lsm_inheritance/fork-shadow-script`
  - `fuzz/corpus/branch_lsm_inheritance/materialization-script`
  - `fuzz/corpus/branch_lsm_install/snapshot-install-script`
  - `fuzz/corpus/branch_lsm_install/compaction-install-script`
- Updated `crates/storage-next/fuzz/.gitignore` to track only the named branch
  LSM scenario seeds while continuing to ignore generated fuzz corpus outputs.
- Added `l6m-assurance-depth-implementation-plan.md`.
- Added `l6m-assurance-depth-test-plan.md`.

### Intentional Boundaries

- L6M does not introduce backend IO, durable recovery records, WAL ordering,
  commit allocation, commit conflict validation, branch registry APIs, branch
  clear/delete APIs, fork-at-history, or public product DTO mapping.
- The branch fuzz targets remain the three L6L targets:
  `branch_lsm_reads`, `branch_lsm_inheritance`, and `branch_lsm_install`.
  L6M deepens the contracts behind those targets instead of adding a fourth
  target.
- The model uses storage-owned row/key constructors and stable encoding helpers,
  but it does not call `BranchReadView` or production candidate selection to
  compute expected visibility.
- Porting-log and sensitivity evidence stays in this document. Runtime tests do
  not assert document paths or prose-only requirements.

### Sensitivity Probe Ledger

| Probe | Mutation | Mutation Site | Expected Failure | Status |
| --- | --- | --- | --- | --- |
| S1 | Sort row-chain commit versions ascending instead of newest-first. | `src/branch/read.rs` candidate ordering and model visibility comparisons. | `branch_lsm_properties` via `check_branch_lsm_reference_model_contract` and `assert_model_store_read_surface`. | Structurally enforced by independent model parity. |
| S2 | Ignore tombstones in latest reads. | `src/branch/read.rs` visible-row filtering. | Direct branch read tests and model-backed latest/history checks. | Structurally enforced. |
| S3 | Evaluate TTL using wall-clock time instead of the requested as-of timestamp. | `src/branch/read.rs` TTL filtering. | Direct timestamp/TTL tests and model-backed timestamp checks. | Structurally enforced. |
| S4 | Omit inherited fork-version gate. | `src/branch/read.rs` inherited candidate collection. | `check_branch_lsm_inheritance_model_contract`; model excludes rows above each layer fork version. | Structurally enforced. |
| S5 | Skip inherited source-to-child branch-id rewrite. | `src/branch/read.rs` inherited candidate rewrite path. | `check_branch_lsm_inheritance_model_contract`; model groups rewritten keys under the child branch. | Structurally enforced. |
| S6 | Search inherited layers before child-local state. | `src/branch/read.rs` source precedence. | Child put shadowing checks in direct tests and model-backed inheritance checks. | Structurally enforced. |
| S7 | Let child-local tombstones fall through to inherited puts. | `src/branch/read.rs` source precedence and tombstone filtering. | Child tombstone shadowing checks in direct tests and model-backed inheritance checks. | Structurally enforced. |
| S8 | Remove an inherited layer before replacement tables are visible. | `src/branch/state.rs` materialization stage/swap path. | Direct materialization retry/idempotency tests and model-backed materialization parity. | Structurally enforced for L6-local atomic state changes. |
| S9 | Mark a table releasable while another branch or inherited layer references it. | `src/branch/state.rs` reachability/release planning. | Direct reachability/release tests and L6L closeout category checks. | Structurally enforced for L6-local release facts. |
| S10 | Let compaction drop old versions without a retention proof. | `src/branch/state.rs` branch compaction policy validation. | Direct branch compaction rejection tests. | Structurally enforced. |
| S11 | Let compaction drop tombstones without proving no resurrection. | `src/branch/state.rs` branch compaction policy validation. | Direct branch compaction rejection tests. | Structurally enforced. |
| S12 | Accept snapshot row branch mismatch. | `src/branch/state.rs` snapshot install validation. | Direct snapshot install invalid-row tests. | Structurally enforced. |
| S13 | Accept duplicate snapshot internal keys. | `src/branch/state.rs` snapshot install validation. | Direct snapshot duplicate rejection tests. | Structurally enforced. |
| S14 | Mutate one snapshot target before later target validation fails. | `src/branch/state.rs` snapshot install preflight/build staging. | Direct all-or-nothing snapshot install tests and `check_branch_lsm_install_model_contract`. | Structurally enforced. |
| S15 | Reintroduce `VersionedValue`, product `Value`, product `Key`, or similar old DTO vocabulary into production branch code. | `src/branch/**/*.rs`. | `branch_lsm_source_guard`. | Source-guard enforced. |
| S16 | Import `crate::commit`, `crate::lifecycle`, `crate::backend`, `crate::service`, object layout, filesystem, environment, wall-clock, or StrataHub vocabulary in production L6 code. | `src/branch/**/*.rs`. | `branch_lsm_source_guard`. | Source-guard enforced. |

### Fault Window Classification

- Fork-reachability-before-visibility is not an L6-local durable IO window.
  L6 can produce inherited-layer state transitions and reachability facts, but
  it does not publish branch manifests or expose a durable two-phase fork
  operation. The ambiguous durable window is deferred to L8, where manifest
  publication and recovery records exist.

### Verification

- `cargo test -p strata-storage-next --locked --lib branch` passed.
- `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` passed.
- `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` passed.
- `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` passed.
- `cargo test -p strata-storage-next --locked --test branch_lsm_closeout` passed.
- `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` passed.
- `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` passed.
- `cargo test -p strata-storage-next --locked` passed as an extra full-package
  regression check.
- `cargo fmt --package strata-storage-next --check` passed.
- `git diff --check` passed.
- Optional/manual fuzz smoke:
  - `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_reads -- -max_total_time=60`
  - `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_inheritance -- -max_total_time=60`
  - `cd crates/storage-next/fuzz && cargo +nightly fuzz run branch_lsm_install -- -max_total_time=60`
  - Not run in this implementation pass; target registration, dedicated
    contract calls, and enriched corpora are enforced by
    `branch_lsm_closeout.rs`.

### Result

L6M turns the L6 generated harness from broad scenario coverage into
model-backed assurance for the highest-risk branch semantics: inherited
visibility, fork gates, branch-id rewriting, child-local shadowing,
materialization parity, snapshot install, and compaction install. Remaining
durable fault windows are explicitly owned by L8.
