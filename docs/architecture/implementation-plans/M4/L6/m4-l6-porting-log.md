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
- Optional `branch_lsm_inheritance` fuzz target remains deferred until the L6
  fuzz inventory slice. The generated branch-LSM property harness now covers
  the L6F inheritance categories with dedicated counters/scripts.

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
