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
manifest) fall back to single-branch mode. **Still outstanding for B Phase
2**: multi-branch WAL/checkpoint encoding (today only the seeded branch can
carry rows durably); persisting `pending_releases`; durable-tombstone WAL
guard test; recovery integration smoke; per-branch `TableManifest` for
non-seeded branches.

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
| 5. recovery preserves branch catalog, generation, deleted markers, inherited layers, and fork-at-history facts | Mostly met | B Phase 1 added durable BranchCatalogManifest + recovery rebuild. Multi-branch row state (inherited layers per non-seeded branch) defers to B Phase 2. |
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

### 6. Multi-branch durable persistence and recovery missing

The database manifest still tracks a single branch.
`LifecycleRecoveryRuntime` returns `recovered_branch: Option<BranchLocalState>`
(recovery.rs:839, 845, 875) — singular. Bootstrap creates the catalog with
`LifecycleBranchCatalog::with_existing_branch(...)` (bootstrap.rs:131) which
only seeds one entry. There is no path that persists multi-branch catalog
state, deleted markers, or fork-at-history descriptors, and no path that
rebuilds them on recovery.

This blocks all of test plan § 7 Recovery (14 tests) and the
durable-tombstone, recovery-generation, and recovery-fork-at-version tests in
§ 3, § 5, § 6.

This is the largest single gap. It likely needs its own slice; would touch:

- Database manifest format (frozen at M3 — extension needed).
- Recovery flow.
- Durable catalog publication for create / fork / clear / delete.

### 7. Test coverage gaps

Counts are "test plan name → present in shipped code" (after A1 + A2 Phase 1).

| Section | Required | Present | Missing | A2 Phase 1 delta |
|---|---:|---:|---:|---:|
| § 1 Catalog Create and List | 10 | 10 | 0 | — |
| § 2 Current-State Fork | 12 | 12 | 0 | — |
| § 3 Fork At History | 12 | 11 | 1 (`recovered_child_keeps_requested_fork_version`; Gap 6) | — |
| § 4 Clear Branch | 12 | 11 | 1 (no backend delete; needs mock backend) | +1 |
| § 5 Delete Branch | 12 | 8 | 4 (deleting state ×3, durable tombstone) | +2 |
| § 6 Generation Reuse | 10 | 9 | 1 (recovery generation; Gap 6) | — |
| § 7 Recovery | 14 | 0 | 14 (Gap 6) | — |
| § 8 Maintenance Interactions | 10 | 1 | 9 (retention drains pending_releases verified) | +1 |
| § 9 Inter-Branch Isolation | 10 | 10 | 0 | +1 |
| § 10 Pinned View Reachability | 10 | 9 | 1 (partial-transition observability; C) | — |
| Cache Mode | 8 | 9 | -1 (Phase 1 added 3 runtime clear/delete tests; over-quota) | +3 |
| Durable Mode | 10 | 3 | 7 (Gaps 1 Phase 2 + 6) | +2 |
| Generated Model | 8 | 0 | 8 | — |
| Fault Windows | 12 | 0 | 12 | — |
| Fuzz Targets | 4 | 0 | 4 | — |
| Integration | 8 | 6 | 2 (durable smoke, recovery smoke) | — |
| **Total** | **162** | **99** | **63** | **+10** |

A2 Phase 1 added 10 plan-required tests (5 §4/§5 catalog-direct, 1 §8
retention drain, 1 §9 shared-table, 3 §11 cache runtime clear/delete) plus
2 durable runtime pending_releases tests. **Phase 2 (shadow-field removal)
is structural; it does not unblock additional test coverage** — the
~6 method signature changes affect existing call sites only.
Follow-up B unblocks § 7 and durable tombstone tests; Follow-up C unblocks
partial-transition tests and the generated/fault/fuzz lines.

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

- Gap 6 (multi-branch durable manifest + recovery)
- Test gaps in § 5 (durable tombstone), § 6 (recovery generation), § 7
  (recovery), Durable Mode, recovery integration smoke

Touches the M3-frozen manifest format. Likely needs to coordinate with L8Q /
L8R owners and the format golden vectors.

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
