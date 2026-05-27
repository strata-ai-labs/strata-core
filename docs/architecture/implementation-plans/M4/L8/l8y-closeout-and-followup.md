# L8Y Closeout and Follow-Up

Status: **temporary working document**

This is a scratch closeout for the L8Y branch lifecycle completeness slice. It
captures gaps found during post-implementation review so they can be addressed
in one or more focused follow-up slices before L9.

**Update (Follow-up A1 landed)**: Follow-up A was split into A1 (no shadow-field
removal, no new clear/delete runtime methods) and A2 (the runtime mutation
wiring + release-plan buffer). A1 landed; the table below reflects A1's effect
on each gap. The plan for A1 lives at
`/Users/aniruddhajoshi/.claude/plans/dapper-hatching-chipmunk.md` (working
copy).

**Update (Follow-up B Phase 1 landed)**: B Phase 1 persists the catalog
descriptor list durably. New `BranchCatalogManifest` format (STBC magic,
version 1, sorted entry list with status/generation/optional parent and
timestamps) lives at `manifest/branch-catalog`. The durable runtime
publishes a fresh manifest after every catalog mutation, tracking a
monotonic `branch_catalog_sequence`. Recovery loads the manifest in
`complete_recovery` and rebuilds the in-memory catalog (Active branches via
`create_branch`, Deleted via `create_branch + delete_branch` with the
persisted `deleted_at`). 11 format tests, 4 golden vectors, 3 § 7 recovery
tests (`recovery_rebuilds_multiple_branch_descriptors`,
`_deleted_marker_outranks_older_table_manifest`,
`_newer_generation_outranks_older_deleted_marker`). Pre-B databases (no
manifest) fall back to single-branch mode.

**Update (Follow-up B Phase 2 landed)**: B Phase 2 extends recovery to
restore per-branch row state. Changes:

- `validate_recovered_wal_package` is now catalog-aware: accepts records
  for any non-Deleted branch known to the catalog; rejects records for
  Deleted or unknown branch_ids with typed `RecoveryFailed` errors.
- `complete_recovery` reorders to build the catalog (seeded slot +
  `BranchCatalogManifest` replay) before validating and replaying the
  WAL.
- New helper `recover_per_branch_table_manifests` enumerates
  `TableManifestService::load_all_current()` and installs each
  non-seeded branch's state into the catalog via the new
  `apply_loaded_table_manifest_to_branch` factored out of
  `recover_table_manifest_for_branch`.
- WAL replay loop dispatches by `branch_id`, looking up the target slot
  via `branch_catalog.branch_state_mut(branch_id, guard)`.
- `replay_branch_catalog_manifest` now preserves parent metadata via a
  new `LifecycleBranchCatalog::set_parent_for_recovery` helper, so
  forked descriptors survive restart with their `source_branch_id` and
  `fork_version`.

§ 7 tests added: `recovery_rebuilds_active_branch_states`,
`recovery_rejects_wal_row_for_deleted_generation`,
`recovery_rebuilds_fork_at_history_version`,
`recovery_table_manifest_multi_branch_rows_round_trip`. The existing
`bootstrap_rejects_recovered_log_record_for_unopened_branch` covers
test § 7.7 (`recovery_rejects_wal_row_for_missing_branch`); its expected
message updated to "references an unknown branch".

**Deferred to B Phase 3**: multi-branch checkpoint encoder (`SnapshotService`
still writes seeded-branch rows only — `validate_checkpoint_rows` stays
single-branch); `recovery_checkpoint_multi_branch_rows_round_trip` test
(needs encoder); `recovery_rebuilds_inherited_layers` test (needs a
non-seeded-branch flush helper); persisting `pending_releases`; recovery
integration smoke (existing lib coverage is end-to-end durable but not a
testkit-style integration harness).

**Update (Follow-up A3 landed)**: A3 dropped the shadow `branch: BranchLocalState`
field on both runtimes. The catalog is now the sole owner of branch state.
A3 was structural cleanup — zero test edits required. The runtime stores an
`initial_branch_id: BranchId` field at open time; the public `branch_state()`
and `branch_state_mut()` accessor signatures are unchanged but now delegate
to the catalog via `branch_catalog.branch_state(initial_branch_id)` (and the
mut equivalent). All 7 internal runtime methods (rotate / flush_frozen /
compact / materialize / storage_pressure / read_view / budget_snapshot),
both commit paths, and the close drain runner now fetch from the catalog.
`sync_branch_catalog` and `mirror_branch_to_shadow` deleted. Gap 1 is now
fully closed.

**Update (Follow-up A2 Phase 1 landed)**: A2 was further split into Phase 1
(transition shape — catalog-authoritative for commits/maintenance via reverse
sync) and Phase 2 (shadow field removal). Phase 1 shipped, then A3 shipped
Phase 2. A2 Phase 1 shipped:

- `pending_releases: Vec<BranchReleasePlan>` field on
  `LifecycleDurableLocalRuntime`.
- `clear_branch` and `delete_branch` methods on both runtimes; durable
  appends release plans to the buffer and emits a telemetry-class
  `RecoveryHealth::Degraded` when `protected_tables()` is non-empty.
- Cache mode discards release plans (no retention pass).
- `DurableRetentionMaintenanceRunner` drains `pending_releases` for
  `Global` and `TableObjects { branch_id }` scopes; drained releases are
  appended to the maintenance outcome's `affected_object_names` as
  `branch-release:<table-identity>` entries. Physical reclaim still
  defers to Follow-up B (durable manifest update).
- 5 mutating maintenance runners now fetch `&mut BranchLocalState` from
  the catalog (cache flush/compaction/materialization/close; durable
  flush/compaction/materialization). Pre-sync (shadow→catalog) at entry
  preserves test patterns that mutate via `branch_state_mut(#[cfg(test)])`;
  post-mirror (catalog→shadow) keeps `branch_state()` readers consistent.
- Commit paths (`execute_cache_commit`, `execute_durable_commit`) route
  through the catalog using `LifecycleBranchCatalog::branch_state_mut_with_registry`
  (new split-borrow accessor) for the batch's branch_id.

Parent docs:

- `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-implementation-plan.md`
- `docs/architecture/implementation-plans/M4/L8/l8y-branch-lifecycle-completeness-test-plan.md`
- `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` (§ L8Y - Branch Lifecycle Completeness)

## L8Y As Shipped

L8Y delivered a storage-internal `LifecycleBranchCatalog`
(`crates/storage-next/src/lifecycle/branch_lifecycle.rs`) with create / list /
fork / fork-at-retained-version / clear / delete / pinned-reachability, plus 72
catalog-direct unit tests, 4 integration smokes, and a
`lifecycle_branch_lifecycle_source_stays_storage_internal` source guard. All
shipped tests pass; clippy and fmt are clean.

The catalog is well-built at the data-structure level: atomic mutations,
generation-safe stale-descriptor CAS, pinned-view ref-counted retention with
dedup, deterministic listing, error-coded outcomes.

## Exit Gate Status (post-A1)

| Gate | Status | Notes |
|---|---|---|
| 1. create/list/clear/delete/fork/fork-at-history work in cache and durable local modes | Met | A1 exposed create / list / fork. A2 Phase 1 added clear / delete. A3 dropped the shadow field — catalog is sole owner. |
| 2. typed errors for duplicate create, missing source, non-empty destination, stale generation, deleted branch, unretained fork version | Met | A1 added `SourceHasUnflushedRows`, `InsufficientTimestampHistory`, `PinnedViewReleaseBlocked` and used `SourceHasUnflushedRows` in fork rejections. |
| 3. pinned read views remain valid across clear/delete/fork and protect reachability | Met | |
| 4. stale flush/compaction/materialization tasks cannot resurrect cleared or deleted rows | Met | Stale descriptor CAS works. |
| 5. recovery preserves branch catalog, generation, deleted markers, inherited layers, and fork-at-history facts | Mostly met | B Phase 1 added durable BranchCatalogManifest + descriptor rebuild. B Phase 2 added catalog-aware WAL validation + per-branch WAL replay dispatch + per-branch TableManifest recovery + parent metadata preservation. Multi-branch checkpoint encoder + inherited_layers test defer to B Phase 3. |
| 6. table-object retention receives release facts; branch lifecycle never directly deletes table objects | Met | |
| 7. source guards prevent product policy and milestone labels in code/tests | Met | |
| 8. generated/fault tests cover branch lifecycle ordering, not only examples | Not met | Generated model, fault windows, and fuzz targets all missing. Follow-up C. |
| 9. full slice command matrix recorded in porting log | Met | |

## Gaps

### 1. Catalog is shadow state, not wired into runtimes — *Closed by A1 + A2 + A3*

A1 added `create_branch`, `list_branches`, and the fork family.
A2 Phase 1 added `clear_branch` and `delete_branch`, the durable
`pending_releases` buffer, retention drain wiring, and routed the 5
mutating maintenance runners + both commit paths through the catalog.
A3 dropped the shadow `branch: BranchLocalState` field, deleted
`sync_branch_catalog` and `mirror_branch_to_shadow`, and added an
`initial_branch_id: BranchId` field as the default-branch anchor. The
public `branch_state()` / `branch_state_mut()` accessor signatures stay
unchanged but delegate to the catalog. All 7 internal runtime methods,
both commit paths, and the durable close drain runner fetch from the
catalog. The catalog is the sole owner of branch state.

References:

- `crates/storage-next/src/lifecycle/cache.rs:48,121,196,471` — catalog stored and synced, but only `branch_catalog() -> &LifecycleBranchCatalog` is exposed (immutable).
- `crates/storage-next/src/lifecycle/durable/bootstrap.rs:34,131,295,339` — same shape on the durable side.

Fix shape: add runtime methods that delegate to the catalog (`create_branch`,
`clear_branch`, `delete_branch`, `fork_current`,
`fork_at_retained_version`). Each must enforce lifecycle admission, route
release facts through retention, and update open-outcome stats.

### 2. Duplicate registry between runtime and catalog — *Closed by A1*

A1 dropped the `registry: CommitBranchRegistry` field from both
`LifecycleCacheRuntime` and `LifecycleDurableLocalRuntime`. The runtimes now
borrow `self.branch_catalog.registry()` everywhere admission is gated. The
catalog is the single source of truth for branch generation state.

### 3. Missing typed error variants — *Partially closed by A1*

A1 added three typed variants:

- `SourceHasUnflushedRows` (code `failed_precondition.lifecycle.fork_source_unflushed`).
  Emitted from `fork_current` precondition check. The L8Y test
  `fork_current_source_with_active_rows_is_rejected_explicitly` now asserts
  this typed variant directly.
- `InsufficientTimestampHistory` (code
  `failed_precondition.lifecycle.timestamp_history`). Emitted from the new
  `fork_at_retained_timestamp` path.
- `PinnedViewReleaseBlocked` (code
  `failed_precondition.lifecycle.pinned_view_release`). Declared in A1; first
  emission site lands in A2 with the release-plan buffer.

Still outstanding:

- `BranchDeleting` — depends on Gap 4 (Clearing/Deleting observable state) and
  defers to Follow-up C.
- `BranchManifestMismatch` and `BranchRecoveryConflict` — recovery-specific
  and defer to Follow-up B.

### 4. Clearing / Deleting states are never observable

`clear_branch` (branch_lifecycle.rs:521–524) and `delete_branch`
(branch_lifecycle.rs:558–559) set the intermediate state and immediately
overwrite it in the same synchronous block. The Clearing/Deleting status is
never visible from outside the function. As a consequence, plan-required tests

- `delete_branch_marks_deleting_before_release`
- `delete_branch_rejects_new_commits_while_deleting`
- `delete_branch_rejects_new_maintenance_while_deleting`

cannot be written against the current implementation and are absent.

Fix shape: pick one and stick with it.

- Option A: refactor to a begin/commit/abort handle pattern so the
  intermediate state is published, observed, and then advanced. Matches the
  plan's State-Transition Rules verbatim.
- Option B: drop the intermediate states from the spec; document that
  storage-internal clear/delete are atomic synchronous transitions and the
  three tests above are not applicable.

### 5. Timestamp coverage missing in fork-at-history — *Closed by A1*

A1 added `LifecycleBranchCatalog::fork_at_retained_timestamp` and
`BranchLocalState::resolve_timestamp_to_commit_version`. The catalog method
validates timestamp coverage (`Unknown` → reject, `CompleteSince` →
range-check), resolves the timestamp to a commit version via the
timeline tiebreaker (highest commit version with `commit_timestamp ≤ T`),
and delegates to `fork_at_retained_version`. Tests:

- `fork_at_history_missing_timestamp_coverage_rejects` (Unknown coverage)
- `fork_at_history_below_timestamp_coverage_rejects` (CompleteSince floor)
- `fork_at_history_timestamp_lookup_uses_timeline_tiebreaker`
- `fork_at_history_no_rows_at_or_before_timestamp_rejects`
- `durable_runtime_fork_at_retained_timestamp_resolves_via_coverage`

### 6. Multi-branch durable persistence and recovery — *Mostly closed by B Phase 1 + 2*

B Phase 1 added the `BranchCatalogManifest` (top-level
`manifest/branch-catalog` object, STBC magic, version 1) and recovery
rebuild for non-seeded descriptors.

B Phase 2 closed the row-state side: WAL validator + replay loop now
dispatch by `branch_id`; persisted per-branch `TableManifest` objects
are enumerated and installed into the catalog; forked descriptors
preserve their parent metadata across restart.

**Still outstanding (B Phase 3)**: the snapshot/checkpoint encoder
still emits seeded-branch rows only (`encode_checkpoint_row_section`
filters by the open branch). `validate_checkpoint_rows` is therefore
left at the single-branch read shape because the catalog is not
available at checkpoint-load time and no rows for non-seeded branches
would survive the encoder anyway. Multi-branch checkpoint encoding +
read-side dispatch + the
`recovery_checkpoint_multi_branch_rows_round_trip` test land together
in B Phase 3.

Also outstanding for B Phase 3: persisting `pending_releases`,
inherited-layer round-trip test (needs a non-seeded-branch flush
helper for the runtime).

### 7. Test coverage gaps

Counts are "test plan name → present in shipped code" (after A1 + A2 Phase 1).

| Section | Required | Present | Missing | A2 Phase 1 delta | B Phase 1/2 delta |
|---|---:|---:|---:|---:|---:|
| § 1 Catalog Create and List | 10 | 10 | 0 | — | — |
| § 2 Current-State Fork | 12 | 12 | 0 | — | — |
| § 3 Fork At History | 12 | 12 | 0 | — | +1 (`recovery_rebuilds_fork_at_history_version`) |
| § 4 Clear Branch | 12 | 11 | 1 (no backend delete; needs mock backend) | +1 | — |
| § 5 Delete Branch | 12 | 9 | 3 (deleting state ×3) | +2 | +1 (`recovery_rejects_wal_row_for_deleted_generation`) |
| § 6 Generation Reuse | 10 | 9 | 1 (recovery generation) | — | — |
| § 7 Recovery | 14 | 7 | 7 (`recovery_rebuilds_inherited_layers`, `recovery_checkpoint_multi_branch_rows_round_trip`, `recovery_reconciles_creating_branch`, `_clearing_branch`, `_deleting_branch`, `recovery_preserves_branch_release_facts`, integration smoke) | — | +7 (3 in B Phase 1 + 4 in B Phase 2) |
| § 8 Maintenance Interactions | 10 | 1 | 9 | +1 | — |
| § 9 Inter-Branch Isolation | 10 | 10 | 0 | +1 | — |
| § 10 Pinned View Reachability | 10 | 9 | 1 (partial-transition observability; C) | — | — |
| Cache Mode | 8 | 9 | -1 | +3 | — |
| Durable Mode | 10 | 3 | 7 | +2 | — |
| Generated Model | 8 | 0 | 8 | — | — |
| Fault Windows | 12 | 0 | 12 | — | — |
| Fuzz Targets | 4 | 0 | 4 | — | — |
| Integration | 8 | 6 | 2 (durable smoke, recovery smoke) | — | — |
| **Total** | **162** | **108** | **54** | **+10** | **+9** |

A2 Phase 1 added 10 plan-required tests (5 §4/§5 catalog-direct, 1 §8
retention drain, 1 §9 shared-table, 3 §11 cache runtime clear/delete) plus
2 durable runtime pending_releases tests. **Phase 2 (shadow-field removal)
is structural; it does not unblock additional test coverage** — the
~6 method signature changes affect existing call sites only.

B Phase 1 added 3 § 7 descriptor-rebuild tests. B Phase 2 added 4 more
§ 7 tests (active branch states, deleted-generation rejection, fork at
history version, multi-branch TableManifest round trip), plus § 3 fork
at history rebuild and § 5 deleted-generation rejection. Total § 7
coverage is now 7/14 (50%).

The remaining 7 § 7 tests split as:
- 1 needs B Phase 3 multi-branch checkpoint encoder
  (`recovery_checkpoint_multi_branch_rows_round_trip`).
- 1 needs B Phase 3 non-seeded flush helper
  (`recovery_rebuilds_inherited_layers`).
- 3 need Follow-up C observable Clearing/Deleting states
  (`recovery_reconciles_creating_branch`, `_clearing_branch`,
  `_deleting_branch`).
- 1 needs B Phase 3 release-facts persistence
  (`recovery_preserves_branch_release_facts`).
- 1 integration smoke (`lifecycle_branch_lifecycle_recovery_smoke`).

Follow-up C unblocks partial-transition tests and the
generated/fault/fuzz lines.

### 8. Minor implementation issues — *Closed by A1*

- `fork_current_nonempty_destination_rejects` was renamed to
  `fork_current_existing_destination_with_rows_rejects_without_overwrite` and
  now also asserts that the existing destination's rows are preserved
  (non-destructive rejection).
- `LifecycleBranchCatalog::with_initial_branch` now accepts
  `created_at: Option<CommitVersion>`. Existing test/testkit callers updated.
- `require_generation`'s `NotSupplied` arm now rejects with
  `BranchGenerationMismatch`. `sync_active_branch_state` was restructured to
  bypass the guard via its own internal path (no longer routes through
  `replace_active_branch_state` with `NotSupplied`), so the dangerous bypass
  is no longer reachable from any caller.

## Suggested Follow-Up Scope

One follow-up slice would be large. A natural three-way split:

### Follow-up A — Runtime integration (no format changes)

- Gap 1 (runtime wiring)
- Gap 2 (registry unification)
- Gap 3 (typed error variants)
- Gap 5 (timestamp coverage)
- Gap 8 (minor cleanups)
- Test gaps in § 3 (timestamp coverage tests), § 4, § 5 (the non-recovery, non-deleting-state ones), § 9, § 10, Cache Mode, Integration (the two non-recovery ones)

This is the highest-value, lowest-risk piece. After it lands, the catalog is
actually authoritative through the runtimes, and ~50 of the missing tests
become writable.

### Follow-up B — Multi-branch durable persistence and recovery

Split into three phases:

**Phase 1 (landed):** `BranchCatalogManifest` durable format,
publication on every catalog mutation, descriptor rebuild on
recovery, 4 golden vectors, 3 § 7 descriptor tests.

**Phase 2 (landed):** Catalog-aware WAL validator + per-branch WAL
replay dispatch; per-branch `TableManifest` recovery via
`load_all_current()`; parent metadata preservation in catalog
rebuild; 4 new § 7 tests.

**Phase 3 (deferred):** Multi-branch checkpoint encoder (snapshot
container writes rows from all active branches) and read-side
dispatch; non-seeded-branch flush helper for the runtime; persist
`pending_releases` across restart so retention drains pick up
where they left off. Unblocks the remaining 4 of 7 missing § 7
tests (`_checkpoint_multi_branch_rows_round_trip`,
`_rebuilds_inherited_layers`, `_preserves_branch_release_facts`,
`recovery_smoke` integration).

### Follow-up C — Lifecycle state observability and assurance

- Gap 4 (Clearing/Deleting observability — Option A or B)
- Generated model, fault windows, fuzz targets

If Option B is chosen for Gap 4, the three deleting-state tests are dropped
from scope; otherwise they land here.

Coding suggestion: L8AA / L8AB / L8AC, or fold into L9 setup. The naming is
not load-bearing; the split is.

## Open Questions

1. **V1 scope for multi-branch durable recovery**: required before L9, or can
   L9 ship with single-branch persistence and L10 add the rest?
2. **Clearing/Deleting observable states**: is this a hard plan invariant or
   can the plan accept atomic synchronous transitions and drop the three
   related tests?
3. **`LifecycleBranchCatalog` ownership model**: should the catalog be the
   single owner of `BranchLocalState`, or stay a parallel mirror of the
   runtime's `branch` field after runtime wiring lands?
4. **Format extension**: does Gap 6 require a new manifest format version
   gated by new golden vectors, or can it ride a backward-compatible
   extension?

## How To Close This Document

Once the follow-up slices are formalized (plan + test plan committed under
`docs/architecture/implementation-plans/M4/L8/` or equivalent), this file
should be deleted and references to it removed from any working notes.
